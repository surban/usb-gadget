//! ZLP (Zero-Length Packet) tests for issue #17.
//!
//! Each test sets up a gadget and runs both device and host sides
//! in separate threads within the same process.

use bytes::{Bytes, BytesMut};
use nusb::{
    transfer::{Buffer, Bulk, Direction, In, Out},
    MaybeFuture,
};
use std::{
    io::ErrorKind,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use usb_gadget::{
    default_udc,
    function::custom::{Custom, Endpoint, EndpointDirection, EndpointReceiver, EndpointSender, Interface},
    Class, Config, Gadget, Id, Strings,
};

use crate::common::*;

const TIMEOUT: Duration = Duration::from_secs(5);
const VID: u16 = 0x1234;
const PID: u16 = 0x0010;

/// Sets up a custom USB gadget with one bulk IN and one bulk OUT endpoint.
fn setup_gadget() -> (usb_gadget::RegGadget, Custom, EndpointReceiver, EndpointSender) {
    let (ep_rx, ep_rx_dir) = EndpointDirection::host_to_device();
    let (ep_tx, ep_tx_dir) = EndpointDirection::device_to_host();

    let (custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 2), "ZLP test interface")
                .with_endpoint(Endpoint::bulk(ep_rx_dir))
                .with_endpoint(Endpoint::bulk(ep_tx_dir)),
        )
        .build();

    let udc = default_udc().expect("cannot get UDC");
    let reg = Gadget::new(
        Class::vendor_specific(255, 0),
        Id::new(VID, PID),
        Strings::new("test", "ZLP test device", "zlp-test-001"),
    )
    .with_config(Config::new("config").with_function(handle))
    .bind(&udc)
    .expect("cannot bind to UDC");

    (reg, custom, ep_rx, ep_tx)
}

/// Opens the ZLP test device on the USB host and claims the interface.
fn open_device() -> (nusb::Interface, nusb::Endpoint<Bulk, In>, nusb::Endpoint<Bulk, Out>, usize, usize) {
    let dev_info = find_device_with_id(VID, PID);
    let device = dev_info.open().wait().expect("cannot open device");
    let cfg = device.active_configuration().expect("no active configuration");

    let mut if_num = None;
    let mut ep_in_addr = None;
    let mut ep_out_addr = None;
    let mut ep_in_mps = 0usize;
    let mut ep_out_mps = 0usize;

    for desc in cfg.interface_alt_settings() {
        for ep in desc.endpoints() {
            match ep.direction() {
                Direction::In => {
                    ep_in_addr = Some(ep.address());
                    ep_in_mps = ep.max_packet_size();
                }
                Direction::Out => {
                    ep_out_addr = Some(ep.address());
                    ep_out_mps = ep.max_packet_size();
                }
            }
            if_num = Some(desc.interface_number());
        }
    }

    let if_num = if_num.expect("no interface found");
    let ep_in_addr = ep_in_addr.expect("no IN endpoint found");
    let ep_out_addr = ep_out_addr.expect("no OUT endpoint found");

    let intf = device.claim_interface(if_num).wait().expect("cannot claim interface");
    let ep_in = intf.endpoint::<Bulk, In>(ep_in_addr).expect("cannot open IN endpoint");
    let ep_out = intf.endpoint::<Bulk, Out>(ep_out_addr).expect("cannot open OUT endpoint");

    (intf, ep_in, ep_out, ep_in_mps, ep_out_mps)
}

/// Runs a minimal device event loop until stop is signaled.
fn run_event_loop(mut custom: Custom, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match custom.event_timeout(Duration::from_secs(1)) {
            Ok(_) => {}
            Err(_) if stop.load(Ordering::Relaxed) => break,
            Err(_) => {}
        }
    }
}

// ─── Test 1: Receive ZLP with MPS-sized buffer ──────────────────────
//
// Host sends MPS bytes of 0xAA followed by a ZLP.
// Device reads with MPS-sized buffer → gets data, then ZLP separately.

#[test]
fn zlp_recv_mps_buffer() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, custom, mut ep_rx, _ep_tx) = setup_gadget();
    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        let stop_ev = stop.clone();
        s.spawn(move || run_event_loop(custom, stop_ev));

        s.spawn(|| {
            let mps = ep_rx.max_packet_size().unwrap();
            println!("device: RX MPS={mps}, receiving with MPS-sized buffer");

            let data =
                ep_rx.recv_and_fetch_timeout(BytesMut::with_capacity(mps), TIMEOUT).expect("recv data failed");
            assert_eq!(data.len(), mps, "expected {mps} bytes, got {}", data.len());
            assert!(data.iter().all(|&b| b == 0xAA), "expected all 0xAA");
            println!("device: read {mps} bytes of 0xAA");

            let zlp =
                ep_rx.recv_and_fetch_timeout(BytesMut::with_capacity(mps), TIMEOUT).expect("recv ZLP failed");
            assert_eq!(zlp.len(), 0, "expected ZLP (0 bytes), got {}", zlp.len());
            println!("device: received ZLP");
        });

        let stop_host = stop.clone();
        s.spawn(move || {
            let (_intf, _ep_in, mut ep_out, _, ep_out_mps) = open_device();

            let c = ep_out.transfer_blocking(vec![0xAA_u8; ep_out_mps].into(), TIMEOUT);
            c.status.expect("host: send data failed");
            println!("host: sent {ep_out_mps} bytes of 0xAA");

            let c = ep_out.transfer_blocking(Vec::<u8>::new().into(), TIMEOUT);
            c.status.expect("host: send ZLP failed");
            println!("host: sent ZLP");

            thread::sleep(Duration::from_millis(500));
            stop_host.store(true, Ordering::Relaxed);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

// ─── Test 2: Receive ZLP with oversized buffer (issue #17) ──────────
//
// Host sends MPS bytes of 0xBB followed by a ZLP.
// Device reads with 2×MPS buffer → ZLP is consumed as transfer terminator;
// a second read times out. This documents the behavior from issue #17.

#[test]
fn zlp_recv_large_buffer() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, custom, mut ep_rx, _ep_tx) = setup_gadget();
    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        let stop_ev = stop.clone();
        s.spawn(move || run_event_loop(custom, stop_ev));

        s.spawn(|| {
            let mps = ep_rx.max_packet_size().unwrap();
            let buf_size = mps * 2;
            println!("device: RX MPS={mps}, receiving with oversized buffer ({buf_size} bytes)");

            let data = ep_rx
                .recv_and_fetch_timeout(BytesMut::with_capacity(buf_size), TIMEOUT)
                .expect("recv data failed");
            println!("device: read {} bytes (buffer was {buf_size})", data.len());
            assert_eq!(data.len(), mps, "expected exactly {mps} bytes, got {}", data.len());
            assert!(data.iter().all(|&b| b == 0xBB), "expected all 0xBB");

            // The ZLP is consumed as transfer terminator by the oversized buffer,
            // so a second read must time out (issue #17).
            println!("device: verifying ZLP was consumed as transfer terminator...");
            match ep_rx.recv_and_fetch_timeout(BytesMut::with_capacity(buf_size), Duration::from_secs(2)) {
                Err(e) if e.kind() == ErrorKind::TimedOut => {
                    println!("device: timed out as expected — ZLP consumed as transfer terminator");
                }
                Ok(data) => {
                    panic!("device: expected timeout, but got {} bytes", data.len());
                }
                Err(e) => panic!("device: unexpected error: {e}"),
            }
        });

        let stop_host = stop.clone();
        s.spawn(move || {
            let (_intf, _ep_in, mut ep_out, _, ep_out_mps) = open_device();

            let c = ep_out.transfer_blocking(vec![0xBB_u8; ep_out_mps].into(), TIMEOUT);
            c.status.expect("host: send data failed");
            println!("host: sent {ep_out_mps} bytes of 0xBB");

            let c = ep_out.transfer_blocking(Vec::<u8>::new().into(), TIMEOUT);
            c.status.expect("host: send ZLP failed");
            println!("host: sent ZLP");

            // Wait extra time for the device's second recv to time out.
            thread::sleep(Duration::from_secs(3));
            stop_host.store(true, Ordering::Relaxed);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

// ─── Test 3: Send standalone ZLP ────────────────────────────────────
//
// Device sends a ZLP via Bytes::new(). Host reads and expects 0 bytes.

#[test]
fn zlp_send_empty() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, custom, _ep_rx, mut ep_tx) = setup_gadget();
    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        let stop_ev = stop.clone();
        s.spawn(move || run_event_loop(custom, stop_ev));

        s.spawn(move || {
            ep_tx.flush_timeout(TIMEOUT).ok();
            ep_tx.send_and_flush_timeout(Bytes::new(), TIMEOUT).expect("device: send ZLP failed");
            println!("device: sent ZLP");
        });

        let stop_host = stop.clone();
        s.spawn(move || {
            let (_intf, mut ep_in, _ep_out, ep_in_mps, _) = open_device();

            let c = ep_in.transfer_blocking(Buffer::new(ep_in_mps), TIMEOUT);
            c.status.expect("host: read failed");
            assert_eq!(c.actual_len, 0, "host: expected ZLP (0 bytes), got {}", c.actual_len);
            println!("host: received ZLP");

            stop_host.store(true, Ordering::Relaxed);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

// ─── Test 4: Send MPS data followed by ZLP ──────────────────────────
//
// Device sends MPS bytes of 0xDD then a ZLP. Host reads data, then ZLP.

#[test]
fn zlp_send_data_then_zlp() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, custom, _ep_rx, mut ep_tx) = setup_gadget();
    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        let stop_ev = stop.clone();
        s.spawn(move || run_event_loop(custom, stop_ev));

        s.spawn(move || {
            let mps = ep_tx.max_packet_size().unwrap();
            ep_tx.flush_timeout(TIMEOUT).ok();
            ep_tx.send_and_flush_timeout(vec![0xDD_u8; mps].into(), TIMEOUT).expect("device: send data failed");
            println!("device: sent {mps} bytes of 0xDD");
            ep_tx.send_and_flush_timeout(Bytes::new(), TIMEOUT).expect("device: send ZLP failed");
            println!("device: sent ZLP");
        });

        let stop_host = stop.clone();
        s.spawn(move || {
            let (_intf, mut ep_in, _ep_out, ep_in_mps, _) = open_device();

            let c = ep_in.transfer_blocking(Buffer::new(ep_in_mps), TIMEOUT);
            c.status.expect("host: read 1 failed");
            assert_eq!(c.actual_len, ep_in_mps, "host: expected {ep_in_mps} bytes, got {}", c.actual_len);
            assert!(c.buffer[..c.actual_len].iter().all(|&b| b == 0xDD), "host: expected all 0xDD");
            println!("host: read {} bytes of 0xDD", c.actual_len);

            let c = ep_in.transfer_blocking(Buffer::new(ep_in_mps), TIMEOUT);
            c.status.expect("host: read 2 failed");
            assert_eq!(c.actual_len, 0, "host: expected ZLP (0 bytes), got {}", c.actual_len);
            println!("host: received ZLP");

            stop_host.store(true, Ordering::Relaxed);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

// ─── Test 5: Send ZLP via send_timeout + flush_timeout (issue #17 item 2) ───
//
// Issue #17 reports that send_timeout(Bytes::new()) must be called twice
// to actually transmit a ZLP. This test verifies that a single
// send_timeout + flush_timeout properly delivers a ZLP.

#[test]
fn zlp_send_single_call() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, custom, _ep_rx, mut ep_tx) = setup_gadget();
    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        let stop_ev = stop.clone();
        s.spawn(move || run_event_loop(custom, stop_ev));

        s.spawn(move || {
            let mps = ep_tx.max_packet_size().unwrap();
            // Send MPS data + ZLP using the raw send_timeout + flush_timeout API,
            // exactly as the issue reporter would.
            ep_tx.flush_timeout(TIMEOUT).ok();
            ep_tx.send_timeout(vec![0xEE_u8; mps].into(), TIMEOUT).expect("device: enqueue data failed");
            ep_tx.flush_timeout(TIMEOUT).expect("device: flush data failed");
            println!("device: sent {mps} bytes of 0xEE via send_timeout+flush");

            ep_tx.send_timeout(Bytes::new(), TIMEOUT).expect("device: enqueue ZLP failed");
            ep_tx.flush_timeout(TIMEOUT).expect("device: flush ZLP failed");
            println!("device: sent ZLP via single send_timeout+flush");
        });

        let stop_host = stop.clone();
        s.spawn(move || {
            let (_intf, mut ep_in, _ep_out, ep_in_mps, _) = open_device();

            let c = ep_in.transfer_blocking(Buffer::new(ep_in_mps), TIMEOUT);
            c.status.expect("host: read 1 failed");
            assert_eq!(c.actual_len, ep_in_mps, "host: expected {ep_in_mps} bytes, got {}", c.actual_len);
            assert!(c.buffer[..c.actual_len].iter().all(|&b| b == 0xEE), "host: expected all 0xEE");
            println!("host: read {} bytes of 0xEE", c.actual_len);

            let c = ep_in.transfer_blocking(Buffer::new(ep_in_mps), TIMEOUT);
            c.status.expect("host: read 2 failed");
            assert_eq!(c.actual_len, 0, "host: expected ZLP (0 bytes), got {}", c.actual_len);
            println!("host: received ZLP from single send_timeout call");

            stop_host.store(true, Ordering::Relaxed);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}
