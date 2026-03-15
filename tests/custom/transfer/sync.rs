//! No-timeout synchronous transfer test (recv_and_fetch / send_and_flush).
//!
//! Uses recv_and_fetch / send_and_flush — the exact API reported in issue #21.

use bytes::{Bytes, BytesMut};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use super::*;

const SYNC_ROUNDS: usize = 16;
const VID: u16 = 0x1234;
const PID: u16 = 0x0021;

/// Device side: blocking IO with recv_and_fetch / send_and_flush (no timeout).
fn run_device_sync(
    mut custom: Custom, mut ep_rx: usb_gadget::function::custom::EndpointReceiver,
    mut ep_tx: usb_gadget::function::custom::EndpointSender,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_rx = stop.clone();
    let stop_tx = stop.clone();

    thread::scope(|s| {
        s.spawn(move || {
            let size = ep_rx.max_packet_size().unwrap();
            let mut expected = 0u8;
            for i in 0..SYNC_ROUNDS {
                let data = ep_rx
                    .recv_and_fetch(BytesMut::with_capacity(size))
                    .unwrap_or_else(|e| panic!("device recv_and_fetch round {i} failed: {e}"));
                assert!(
                    data.iter().all(|&x| x == expected),
                    "device recv_and_fetch round {i}: expected all 0x{expected:02x}, got {:02x?}",
                    &data[..data.len().min(16)]
                );
                expected = expected.wrapping_add(1);
            }
            println!("device: recv_and_fetch completed {SYNC_ROUNDS} rounds");
            stop_rx.store(true, Ordering::Relaxed);
        });

        s.spawn(move || {
            let size = ep_tx.max_packet_size().unwrap().min(PACKET_SIZE);
            let mut b = 0u8;
            for i in 0..SYNC_ROUNDS {
                let data: Bytes = vec![b; size].into();
                ep_tx
                    .send_and_flush(data)
                    .unwrap_or_else(|e| panic!("device send_and_flush round {i} failed: {e}"));
                b = b.wrapping_add(1);
            }
            println!("device: send_and_flush completed {SYNC_ROUNDS} rounds");
            stop_tx.store(true, Ordering::Relaxed);
        });

        run_device_events(&mut custom, &stop, false);
    });
}

/// Test recv_and_fetch and send_and_flush — the no-timeout synchronous API
/// reported in issue #21.
#[test]
#[serial]
fn transfer_sync_no_timeout() {
    init();

    if skip_host() {
        return;
    }

    let (vid, pid) = (VID, PID);
    let (reg, custom, ep_rx, ep_tx) =
        setup_gadget_with_id(vid, pid, "sync transfer test device", "sync-transfer-test-001");

    thread::scope(|s| {
        s.spawn(|| run_device_sync(custom, ep_rx, ep_tx));
        s.spawn(|| {
            let (_intf, ep_in, ep_out, _if_num) = open_host_device(vid, pid);
            run_host_bulk(ep_in, ep_out, SYNC_ROUNDS);
            println!("host: all transfers complete");
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}
