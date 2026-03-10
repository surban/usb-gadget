//! Bulk throughput benchmark.
//!
//! Measures USB bulk IN and OUT throughput using large buffers and a deep
//! AIO queue to saturate the USB link.
//!
//! In release mode, measured throughput is checked against a minimum floor
//! that depends on the UDC driver in use (see [`min_throughput_mib_s`]).

use bytes::{Bytes, BytesMut};
use nusb::transfer::{ControlOut, ControlType, Recipient};
use std::{
    io::{ErrorKind, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use usb_gadget::{
    default_udc,
    function::custom::{Custom, Endpoint, EndpointDirection, Interface},
    Class, Config, Gadget, Id, Strings, Speed,
};

use super::*;

const VID: u16 = 0x1234;
const PID: u16 = 0x0022;

/// Total number of bytes to transfer in each direction during the throughput benchmark.
const BENCH_TOTAL_BYTES: usize = 32 * 1024 * 1024;
/// Size of device- and host-side buffers (batches multiple USB packets per URB).
const BENCH_BUF_SIZE: usize = 16384;
/// AIO queue depth for the benchmark endpoints.
const BENCH_QUEUE_LEN: u32 = 32;

/// Minimum expected throughput (MiB/s, per direction) for a given UDC driver
/// and negotiated speed.
///
/// These floors are intentionally conservative — roughly one-third to one-half
/// of the practical wire speed — so that they pass on loaded CI machines and
/// real hardware while still catching gross regressions.
///
/// Returns `None` if no assertion should be made (e.g. low/full speed where
/// throughput is negligible).
///
/// | Driver       | Max speed      | Expected floor | Rationale                              |
/// |--------------|----------------|----------------|----------------------------------------|
/// | `dummy_udc`  | High-Speed     | 100 MiB/s      | Virtual loopback, no real USB wire     |
/// | `dummy_udc`  | Super-Speed+   | 300 MiB/s      | Virtual loopback, no real USB wire     |
/// | `dwc2`       | High-Speed     | 10 MiB/s       | DesignWare USB 2.0 OTG, ~20 MiB/s max |
/// | `dwc3`       | High-Speed     | 10 MiB/s       | DesignWare USB 3.x in HS mode          |
/// | `dwc3`       | Super-Speed+   | 100 MiB/s      | DesignWare USB 3.x at 5+ Gbps          |
/// | `cdns3`      | High-Speed     | 10 MiB/s       | Cadence USB 3.x in HS mode             |
/// | `cdns3`      | Super-Speed+   | 100 MiB/s      | Cadence USB 3.x at 5+ Gbps            |
/// | `musb-hdrc`  | High-Speed     | 5 MiB/s        | Mentor USB 2.0 OTG (TI, Allwinner)    |
/// | (other)      | High-Speed     | 5 MiB/s        | Conservative fallback for real HW      |
/// | (other)      | Super-Speed+   | 50 MiB/s       | Conservative fallback for real HW      |
fn min_throughput_mib_s(driver: &str, max_speed: Speed) -> Option<f64> {
    if max_speed < Speed::HighSpeed {
        return None;
    }

    let is_ss = max_speed >= Speed::SuperSpeed;

    match driver {
        "dummy_udc" => Some(if is_ss { 300.0 } else { 100.0 }),
        "dwc2"      => Some(10.0), // HS only
        "dwc3"      => Some(if is_ss { 100.0 } else { 10.0 }),
        "cdns3"     => Some(if is_ss { 100.0 } else { 10.0 }),
        "musb-hdrc" => Some(5.0),
        _           => Some(if is_ss { 50.0 } else { 5.0 }),
    }
}

/// Sets up a custom USB gadget with deeper AIO queues for benchmarking.
///
/// Returns the registration handle, custom function, endpoint handles,
/// the UDC driver name, and the maximum speed.
fn setup_bench_gadget() -> (
    usb_gadget::RegGadget,
    Custom,
    usb_gadget::function::custom::EndpointReceiver,
    usb_gadget::function::custom::EndpointSender,
    String,
    Speed,
) {
    let (vid, pid) = (VID, PID);
    let (ep_rx, ep_rx_dir) = EndpointDirection::host_to_device();
    let ep_rx_dir = ep_rx_dir.with_queue_len(BENCH_QUEUE_LEN);
    let (ep_tx, ep_tx_dir) = EndpointDirection::device_to_host();
    let ep_tx_dir = ep_tx_dir.with_queue_len(BENCH_QUEUE_LEN);

    let (custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 2), "bench interface")
                .with_endpoint(Endpoint::bulk(ep_rx_dir))
                .with_endpoint(Endpoint::bulk(ep_tx_dir)),
        )
        .build();

    let udc = default_udc().expect("cannot get UDC");

    let driver = udc.driver().expect("cannot query UDC driver").to_string_lossy().into_owned();
    let max_speed = udc.max_speed().expect("cannot query UDC max speed");
    println!("UDC {} driver={driver} max_speed={max_speed}", udc.name().to_string_lossy());

    let reg = Gadget::new(
        Class::new(255, 255, 0),
        Id::new(vid, pid),
        Strings::new("test", "throughput bench device", "bench-001"),
    )
    .with_config(Config::new("config").with_function(handle))
    .bind(&udc)
    .expect("cannot bind to UDC");

    (reg, custom, ep_rx, ep_tx, driver, max_speed)
}

/// Device side for the throughput benchmark: pipelined send/recv with
/// pre-allocated buffers for minimal allocation overhead.
fn run_device_bench(
    mut custom: Custom, mut ep_rx: usb_gadget::function::custom::EndpointReceiver,
    mut ep_tx: usb_gadget::function::custom::EndpointSender, stop: Arc<AtomicBool>,
) {
    let stop_rx = stop.clone();
    let stop_tx = stop.clone();

    thread::scope(|s| {
        // Receive from host (OUT direction).
        s.spawn(move || {
            let size = BENCH_BUF_SIZE;
            let mut pool: Vec<BytesMut> = Vec::new();
            let mut received: usize = 0;

            while !stop_rx.load(Ordering::Relaxed) {
                let buf = pool.pop().unwrap_or_else(|| BytesMut::zeroed(size));
                match ep_rx.recv_timeout(buf, Duration::from_secs(5)) {
                    Ok(Some(data)) => {
                        received += data.len();
                        let mut recycled = data;
                        recycled.clear();
                        recycled.resize(size, 0);
                        pool.push(recycled);
                    }
                    Ok(None) => {}
                    Err(e) if e.kind() == ErrorKind::TimedOut => {}
                    Err(e) if stop_rx.load(Ordering::Relaxed) => {
                        println!("device bench recv stopped: {e}");
                        break;
                    }
                    Err(e) => panic!("device bench recv error: {e}"),
                }
            }
            println!("device: received {received} bytes total");
        });

        // Send to host (IN direction).
        s.spawn(move || {
            let size = BENCH_BUF_SIZE;
            let payload: Bytes = vec![0xABu8; size].into();
            let mut sent: usize = 0;

            while !stop_tx.load(Ordering::Relaxed) {
                match ep_tx.send_timeout(payload.clone(), Duration::from_secs(5)) {
                    Ok(()) => sent += size,
                    Err(e) if e.kind() == ErrorKind::TimedOut => {}
                    Err(e) if stop_tx.load(Ordering::Relaxed) => {
                        println!("device bench send stopped: {e}");
                        break;
                    }
                    Err(e) => panic!("device bench send error: {e}"),
                }
            }
            println!("device: sent {sent} bytes total");
        });

        run_device_events(&mut custom, &stop, true);
    });
}

/// Guard that sets the stop flag on drop, ensuring the device side is
/// signalled even if the host side panics.
struct StopGuard<'a>(&'a AtomicBool);

impl Drop for StopGuard<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Host side for the throughput benchmark: measures bulk IN and OUT throughput
/// over `BENCH_TOTAL_BYTES` bytes in each direction.
fn run_host_bench(stop: &AtomicBool, driver: &str, max_speed: Speed) {
    let _guard = StopGuard(stop);
    let (vid, pid) = (VID, PID);
    let (intf, ep_in, ep_out, if_num) = open_host_device(vid, pid);

    let total_bytes = BENCH_TOTAL_BYTES;

    let (in_bytes, in_elapsed, out_bytes, out_elapsed) = thread::scope(|t| {
        // Read from device (IN).
        let reader_handle = t.spawn(|| {
            let mut reader = ep_in.reader(65536);
            let mut buf = vec![0u8; BENCH_BUF_SIZE];
            let mut total_read: usize = 0;

            let start = Instant::now();
            while total_read < total_bytes {
                let n = reader.read(&mut buf).expect("host bench: read failed");
                total_read += n;
            }
            let elapsed = start.elapsed();
            (total_read, elapsed)
        });

        // Write to device (OUT).
        let writer_handle = t.spawn(|| {
            let mut writer = ep_out.writer(65536);
            let payload = vec![0xCDu8; BENCH_BUF_SIZE];
            let mut total_written: usize = 0;

            let start = Instant::now();
            while total_written < total_bytes {
                let chunk = (total_bytes - total_written).min(BENCH_BUF_SIZE);
                writer.write_all(&payload[..chunk]).expect("host bench: write failed");
                total_written += chunk;
            }
            writer.flush().expect("host bench: flush failed");
            let elapsed = start.elapsed();
            (total_written, elapsed)
        });

        let (in_bytes, in_elapsed) = reader_handle.join().unwrap();
        let (out_bytes, out_elapsed) = writer_handle.join().unwrap();
        (in_bytes, in_elapsed, out_bytes, out_elapsed)
    });

    let in_mib_s = in_bytes as f64 / in_elapsed.as_secs_f64() / (1024.0 * 1024.0);
    let out_mib_s = out_bytes as f64 / out_elapsed.as_secs_f64() / (1024.0 * 1024.0);

    println!();
    println!("=== Bulk throughput benchmark results ===");
    println!(
        "  IN  (device -> host): {in_bytes:>10} bytes in {:>8.3} s = {in_mib_s:>8.2} MiB/s",
        in_elapsed.as_secs_f64()
    );
    println!(
        "  OUT (host -> device): {out_bytes:>10} bytes in {:>8.3} s = {out_mib_s:>8.2} MiB/s",
        out_elapsed.as_secs_f64()
    );
    println!("=========================================");
    println!();

    // In release mode, verify that throughput meets the expected floor for
    // this UDC driver and speed.
    #[cfg(not(debug_assertions))]
    if let Some(min) = min_throughput_mib_s(driver, max_speed) {
        println!("Throughput floor for driver={driver} speed={max_speed}: {min:.0} MiB/s");
        assert!(
            in_mib_s >= min,
            "IN throughput {in_mib_s:.1} MiB/s is below the {min:.0} MiB/s floor \
             (driver={driver}, speed={max_speed})",
        );
        assert!(
            out_mib_s >= min,
            "OUT throughput {out_mib_s:.1} MiB/s is below the {min:.0} MiB/s floor \
             (driver={driver}, speed={max_speed})",
        );
        println!("Throughput floor check passed (>= {min:.0} MiB/s)");
    }

    // Suppress unused variable warnings in debug mode.
    #[cfg(debug_assertions)]
    let _ = (driver, max_speed);

    // Signal device to stop (also done by StopGuard on panic).
    stop.store(true, Ordering::Relaxed);
    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: req::STOP,
            value: 0,
            index: if_num.into(),
            data: &[],
        },
        Duration::from_secs(2),
    )
    .wait()
    .expect("host bench: stop control failed");
}

#[test]
fn throughput_benchmark() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, custom, ep_rx, ep_tx, driver, max_speed) = setup_bench_gadget();
    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        let stop_device = stop.clone();
        s.spawn(move || run_device_bench(custom, ep_rx, ep_tx, stop_device));
        s.spawn(|| run_host_bench(&stop, &driver, max_speed));
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}