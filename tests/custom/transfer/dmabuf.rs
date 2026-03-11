//! DMA-BUF zero-copy loopback transfer test.
//!
//! Allocates a DMA-BUF from `/dev/dma_heap/system` (via the `dma-heap` crate),
//! attaches it to both a bulk OUT (host→device) and bulk IN (device→host)
//! endpoint, then verifies that data written by the host is looped back
//! unmodified through the DMA-BUF.
//!
//! CPU-side access to the buffer uses the `dma-buf` crate for safe mmap +
//! cache-coherency management.
//!
//! Requires kernel ≥ 6.9 and a UDC with scatter-gather support (e.g. dwc3).
//! The test is skipped gracefully when either the DMA heap or UDC sg support
//! is unavailable.

use dma_buf::DmaBuf;
use dma_heap::{Heap, HeapKind};
use rustix::{
    event::{self, PollFd, PollFlags},
    ioctl::{self, opcode, Updater},
};
use std::{
    io,
    os::fd::{AsFd, FromRawFd, OwnedFd},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use super::*;

const VID: u16 = 0x1234;
const PID: u16 = 0x0030;
const BUF_SIZE: usize = 4096;

// ─── Fence helpers (not covered by the dma-buf crate) ───────────────

/// Kernel `struct dma_buf_export_sync_file`.
#[repr(C)]
struct DmaBufExportSyncFile {
    flags: u32,
    fd: i32,
}

const DMA_BUF_SYNC_WRITE: u32 = 2;

/// Exports a sync_file from the DMA-BUF for fence waiting.
///
/// `DMA_BUF_IOCTL_EXPORT_SYNC_FILE`: `_IOWR('b', 2, struct dma_buf_export_sync_file)`
fn dmabuf_export_sync_file(dmabuf: &DmaBuf, flags: u32) -> io::Result<OwnedFd> {
    let mut req = DmaBufExportSyncFile { flags, fd: -1 };
    unsafe {
        ioctl::ioctl(dmabuf, Updater::<{ opcode::read_write::<DmaBufExportSyncFile>(b'b', 2) }, _>::new(&mut req))
    }?;
    Ok(unsafe { OwnedFd::from_raw_fd(req.fd) })
}

/// Waits for a sync_file to signal using `poll()`.
fn wait_sync_file(fd: &OwnedFd, timeout: Duration) -> io::Result<()> {
    let ts = rustix::event::Timespec::try_from(timeout).unwrap();
    let mut pfd = [PollFd::new(fd, PollFlags::IN)];
    let n = event::poll(&mut pfd, Some(&ts))?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "sync_file poll timed out"));
    }
    Ok(())
}

// ─── Device side ────────────────────────────────────────────────────

fn run_device_dmabuf(
    mut custom: Custom, mut ep_rx: usb_gadget::function::custom::EndpointReceiver,
    mut ep_tx: usb_gadget::function::custom::EndpointSender, dmabuf: &DmaBuf,
) {
    let stop = AtomicBool::new(false);

    thread::scope(|s| {
        s.spawn(|| {
            let ctrl_rx = ep_rx.control().unwrap();
            let ctrl_tx = ep_tx.control().unwrap();

            // Attach the DMA-BUF to both endpoints.
            ctrl_rx.dmabuf_attach(dmabuf.as_fd()).unwrap();
            ctrl_tx.dmabuf_attach(dmabuf.as_fd()).unwrap();

            // Queue a receive: host → DMA-BUF.
            ctrl_rx.dmabuf_transfer(dmabuf.as_fd(), BUF_SIZE as u64).unwrap();

            // Wait for the OUT transfer to complete via fence.
            let sync_fd = dmabuf_export_sync_file(dmabuf, DMA_BUF_SYNC_WRITE).unwrap();
            wait_sync_file(&sync_fd, Duration::from_secs(5)).unwrap();
            drop(sync_fd);
            println!("device: DMA-BUF receive complete");

            // Send the same DMA-BUF content back: DMA-BUF → host.
            ctrl_tx.dmabuf_transfer(dmabuf.as_fd(), BUF_SIZE as u64).unwrap();

            // Wait for the IN transfer to complete via fence.
            let sync_fd = dmabuf_export_sync_file(dmabuf, DMA_BUF_SYNC_WRITE).unwrap();
            wait_sync_file(&sync_fd, Duration::from_secs(5)).unwrap();
            drop(sync_fd);
            println!("device: DMA-BUF send complete");

            // Clean up.
            ctrl_rx.dmabuf_detach(dmabuf.as_fd()).unwrap();
            ctrl_tx.dmabuf_detach(dmabuf.as_fd()).unwrap();

            stop.store(true, Ordering::Relaxed);
        });

        run_device_events(&mut custom, &stop, false);
    });
}

// ─── Host side ──────────────────────────────────────────────────────

fn run_host_dmabuf(ep_in: nusb::Endpoint<Bulk, In>, ep_out: nusb::Endpoint<Bulk, Out>) {
    let test_data: Vec<u8> = (0..BUF_SIZE).map(|i| (i % 251) as u8).collect();

    // Send test pattern to device.
    let mut writer = ep_out.writer(4096);
    writer.write_all(&test_data).expect("host: dmabuf write failed");
    writer.flush().expect("host: dmabuf flush failed");
    println!("host: wrote {BUF_SIZE} bytes");

    // Read loopback data back.
    let mut reader = ep_in.reader(4096);
    let mut buf = vec![0u8; BUF_SIZE];
    let mut total = 0;
    while total < BUF_SIZE {
        let n = reader.read(&mut buf[total..]).expect("host: dmabuf read failed");
        assert!(n > 0, "host: unexpected zero-length read");
        total += n;
    }

    assert_eq!(&buf, &test_data, "host: DMA-BUF loopback data mismatch");
    println!("host: DMA-BUF loopback verified ({total} bytes)");
}

// ─── Test entry point ───────────────────────────────────────────────

#[test]
fn transfer_dmabuf() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    // Allocate a DMA-BUF from the system heap — skip if unavailable.
    let heap = match Heap::new(HeapKind::System) {
        Ok(h) => h,
        Err(e) => {
            println!("skipping dmabuf test: cannot open DMA heap: {e}");
            return;
        }
    };
    let dmabuf = match heap.allocate(BUF_SIZE) {
        Ok(fd) => DmaBuf::from(fd),
        Err(e) => {
            println!("skipping dmabuf test: cannot allocate DMA-BUF: {e}");
            return;
        }
    };

    let (reg, custom, mut ep_rx, ep_tx) = setup_gadget_with_id(VID, PID, "dmabuf test device", "dmabuf-test-001");

    // Probe whether the UDC supports scatter-gather (required for DMA-BUF).
    {
        let ctrl = ep_rx.control().unwrap();
        match ctrl.dmabuf_attach(dmabuf.as_fd()) {
            Ok(()) => {
                ctrl.dmabuf_detach(dmabuf.as_fd()).unwrap();
            }
            Err(e) => {
                println!("skipping dmabuf test: UDC lacks scatter-gather support: {e}");
                reg.remove().unwrap();
                return;
            }
        }
    }

    thread::scope(|s| {
        s.spawn(|| run_device_dmabuf(custom, ep_rx, ep_tx, &dmabuf));
        s.spawn(|| {
            let (_intf, ep_in, ep_out, _if_num) = open_host_device(VID, PID);
            run_host_dmabuf(ep_in, ep_out);
        });
    });

    // Verify buffer contents via mmap (cache-coherent access managed by dma-buf crate).
    let mapped = dmabuf.memory_map().unwrap();
    mapped
        .read(
            |data, _: Option<()>| {
                let expected: Vec<u8> = (0..BUF_SIZE).map(|i| (i % 251) as u8).collect();
                assert_eq!(data, expected.as_slice(), "DMA-BUF content mismatch");
                println!("DMA-BUF mmap verified ({BUF_SIZE} bytes)");
                Ok(())
            },
            None,
        )
        .unwrap();

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}
