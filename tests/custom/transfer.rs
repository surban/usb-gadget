//! USB bulk and control transfer tests.
//!
//! Sets up a custom USB gadget with one bulk IN and one bulk OUT endpoint,
//! then performs transfers from both device and host sides in separate threads.
//!
//! Tests synchronous (pipelined), synchronous (no-timeout), and async device-side IO.

use bytes::{Bytes, BytesMut};
use nusb::{
    transfer::{Bulk, ControlIn, ControlOut, ControlType, Direction, In, Out, Recipient},
    MaybeFuture,
};
use std::{
    io::{ErrorKind, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use usb_gadget::{
    default_udc,
    function::custom::{Custom, Endpoint, EndpointDirection, Event, Interface},
    Class, Config, Gadget, Id, Strings,
};

use crate::common::*;

const PACKET_SIZE: usize = 512;
const ROUNDS: usize = 64;
const SYNC_ROUNDS: usize = 16;

/// Vendor request codes.
mod req {
    pub const ECHO: u8 = 1;
    pub const STOP: u8 = 255;
}

/// VID/PID pairs for each test variant (so they can be distinguished on the host).
mod ids {
    pub const TRANSFER: (u16, u16) = (0x1234, 0x0020);
    pub const SYNC: (u16, u16) = (0x1234, 0x0021);
}

// ─── Shared helpers ─────────────────────────────────────────────────

/// Sets up a custom USB gadget with one bulk IN and one bulk OUT endpoint.
fn setup_gadget_with_id(
    vid: u16, pid: u16, product: &str, serial: &str,
) -> (
    usb_gadget::RegGadget,
    Custom,
    usb_gadget::function::custom::EndpointReceiver,
    usb_gadget::function::custom::EndpointSender,
) {
    let (ep_rx, ep_rx_dir) = EndpointDirection::host_to_device();
    let (ep_tx, ep_tx_dir) = EndpointDirection::device_to_host();

    let (custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 2), "transfer test interface")
                .with_endpoint(Endpoint::bulk(ep_rx_dir))
                .with_endpoint(Endpoint::bulk(ep_tx_dir)),
        )
        .build();

    let udc = default_udc().expect("cannot get UDC");
    let reg = Gadget::new(Class::new(255, 255, 0), Id::new(vid, pid), Strings::new("test", product, serial))
        .with_config(Config::new("config").with_function(handle))
        .bind(&udc)
        .expect("cannot bind to UDC");

    (reg, custom, ep_rx, ep_tx)
}

/// Opens a USB device on the host side by VID/PID, claims its interface, and
/// returns the interface handle, bulk IN/OUT endpoints, and the interface number.
fn open_host_device(
    vid: u16, pid: u16,
) -> (nusb::Interface, nusb::Endpoint<Bulk, In>, nusb::Endpoint<Bulk, Out>, u8) {
    let dev_info = find_device_with_id(vid, pid);
    println!("host: found device: {dev_info:?}");

    let device = dev_info.open().wait().expect("host: cannot open device");
    let cfg = device.active_configuration().expect("host: no active configuration");

    let mut if_num = None;
    let mut ep_in_addr = None;
    let mut ep_out_addr = None;
    for desc in cfg.interface_alt_settings() {
        for ep in desc.endpoints() {
            match ep.direction() {
                Direction::In => ep_in_addr = Some(ep.address()),
                Direction::Out => ep_out_addr = Some(ep.address()),
            }
            if_num = Some(desc.interface_number());
        }
    }
    let if_num = if_num.expect("host: no interface found");
    let ep_in_addr = ep_in_addr.expect("host: no IN endpoint");
    let ep_out_addr = ep_out_addr.expect("host: no OUT endpoint");

    let intf = device.claim_interface(if_num).wait().expect("host: cannot claim interface");
    let ep_in = intf.endpoint::<Bulk, In>(ep_in_addr).expect("host: cannot open IN endpoint");
    let ep_out = intf.endpoint::<Bulk, Out>(ep_out_addr).expect("host: cannot open OUT endpoint");

    (intf, ep_in, ep_out, if_num)
}

/// Runs the host-side bulk transfer loop: reads `rounds` packets from the IN
/// endpoint and writes `rounds` packets to the OUT endpoint concurrently.
fn run_host_bulk(ep_in: nusb::Endpoint<Bulk, In>, ep_out: nusb::Endpoint<Bulk, Out>, rounds: usize) {
    thread::scope(|t| {
        // Read from device.
        t.spawn(|| {
            let mut reader = ep_in.reader(4096);
            let mut expected = 0u8;
            for i in 0..rounds {
                let mut buf = vec![0u8; PACKET_SIZE];
                let n = reader.read(&mut buf).expect("host: read failed");
                buf.truncate(n);
                assert!(
                    buf.iter().all(|&x| x == expected),
                    "host: read round {i}: expected all 0x{expected:02x}, got {:02x?}",
                    &buf[..buf.len().min(16)]
                );
                expected = expected.wrapping_add(1);
            }
            println!("host: read {rounds} bulk packets OK");
        });

        // Write to device.
        t.spawn(|| {
            let mut writer = ep_out.writer(4096);
            let mut b = 0u8;
            for _ in 0..rounds {
                writer.write_all(&vec![b; PACKET_SIZE]).expect("host: write failed");
                writer.flush().expect("host: flush failed");
                b = b.wrapping_add(1);
            }
            println!("host: wrote {rounds} bulk packets OK");
        });
    });
}

/// Runs a minimal device event loop until `stop` is signalled. Optionally
/// handles vendor control requests (ECHO / STOP) when `handle_ctrl` is true.
fn run_device_events(custom: &mut Custom, stop: &AtomicBool, handle_ctrl: bool) {
    let mut ctrl_data = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        match custom.event_timeout(Duration::from_secs(1)) {
            Ok(Some(event)) if handle_ctrl => match event {
                Event::SetupHostToDevice(req) => {
                    if req.ctrl_req().request == req::STOP {
                        stop.store(true, Ordering::Relaxed);
                    }
                    ctrl_data = req.recv_all().unwrap();
                }
                Event::SetupDeviceToHost(req) => {
                    if req.ctrl_req().request == req::ECHO {
                        req.send(&ctrl_data).unwrap();
                    }
                }
                _ => {}
            },
            Ok(_) => {}
            Err(e) => {
                if !stop.load(Ordering::Relaxed) {
                    panic!("device event error: {e}");
                }
                break;
            }
        }
    }
}

// ─── Test 1: pipelined synchronous transfers (recv_timeout / send_timeout) ──

/// Device side: pipelined IO with recv_timeout / send_timeout.
fn run_device_pipelined(
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
            while !stop_rx.load(Ordering::Relaxed) {
                match ep_rx.recv_timeout(BytesMut::with_capacity(size), Duration::from_secs(2)) {
                    Ok(Some(data)) => {
                        assert!(
                            data.iter().all(|&x| x == expected),
                            "device recv: expected all 0x{expected:02x}, got {:02x?}",
                            &data[..data.len().min(16)]
                        );
                        expected = expected.wrapping_add(1);
                    }
                    Ok(None) => {}
                    Err(e) if e.kind() == ErrorKind::TimedOut => {}
                    Err(e) if stop_rx.load(Ordering::Relaxed) => {
                        println!("device recv stopped: {e}");
                        break;
                    }
                    Err(e) => panic!("device recv error: {e}"),
                }
            }
        });

        s.spawn(move || {
            let size = ep_tx.max_packet_size().unwrap().min(PACKET_SIZE);
            let mut b = 0u8;
            while !stop_tx.load(Ordering::Relaxed) {
                let data = vec![b; size];
                match ep_tx.send_timeout(data.into(), Duration::from_secs(2)) {
                    Ok(()) => b = b.wrapping_add(1),
                    Err(e) if e.kind() == ErrorKind::TimedOut => {}
                    Err(e) if stop_tx.load(Ordering::Relaxed) => {
                        println!("device send stopped: {e}");
                        break;
                    }
                    Err(e) => panic!("device send error: {e}"),
                }
            }
        });

        run_device_events(&mut custom, &stop, true);
    });
}

/// Host side for the pipelined test: control echo + bulk transfers + STOP.
fn run_host_pipelined() {
    let (vid, pid) = ids::TRANSFER;
    let (intf, ep_in, ep_out, if_num) = open_host_device(vid, pid);

    // --- Control transfer test ---
    let test_data: Vec<u8> = (0..64).collect();
    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: req::ECHO,
            value: 0,
            index: if_num.into(),
            data: &test_data,
        },
        Duration::from_secs(2),
    )
    .wait()
    .expect("host: control out failed");

    let reply = intf
        .control_in(
            ControlIn {
                control_type: ControlType::Vendor,
                recipient: Recipient::Interface,
                request: req::ECHO,
                value: 0,
                index: if_num.into(),
                length: test_data.len() as u16,
            },
            Duration::from_secs(2),
        )
        .wait()
        .expect("host: control in failed");
    assert_eq!(reply.as_slice(), test_data.as_slice(), "host: control echo mismatch");
    println!("host: control echo OK ({} bytes)", test_data.len());

    // --- Bulk transfers ---
    run_host_bulk(ep_in, ep_out, ROUNDS);

    // Signal device to stop.
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
    .expect("host: stop control failed");

    println!("host: all transfers complete");
}

#[test]
fn transfer() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (vid, pid) = ids::TRANSFER;
    let (reg, custom, ep_rx, ep_tx) = setup_gadget_with_id(vid, pid, "transfer test device", "transfer-test-001");

    thread::scope(|s| {
        s.spawn(|| run_device_pipelined(custom, ep_rx, ep_tx));
        s.spawn(run_host_pipelined);
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

// ─── Test 2: no-timeout synchronous transfers (issue #21) ───────────
//
// Uses recv_and_fetch / send_and_flush — the exact API reported in
// issue #21.

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
fn transfer_sync_no_timeout() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (vid, pid) = ids::SYNC;
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

// ─── Test 3: async transfers ────────────────────────────────────────

#[cfg(feature = "tokio")]
async fn run_device_async(
    mut custom: Custom, mut ep_rx: usb_gadget::function::custom::EndpointReceiver,
    mut ep_tx: usb_gadget::function::custom::EndpointSender,
) {
    use tokio::sync::Notify;

    let stop = Arc::new(Notify::new());

    let stop_rx = stop.clone();
    let rx_task = tokio::spawn(async move {
        let size = ep_rx.max_packet_size().unwrap();
        let mut expected = 0u8;
        loop {
            tokio::select! {
                result = ep_rx.recv_async(BytesMut::with_capacity(size)) => {
                    match result {
                        Ok(Some(data)) => {
                            assert!(
                                data.iter().all(|&x| x == expected),
                                "device async recv: expected all 0x{expected:02x}, got {:02x?}",
                                &data[..data.len().min(16)]
                            );
                            expected = expected.wrapping_add(1);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            println!("device async recv stopped: {e}");
                            break;
                        }
                    }
                }
                _ = stop_rx.notified() => break,
            }
        }
    });

    let stop_tx = stop.clone();
    let tx_task = tokio::spawn(async move {
        let size = ep_tx.max_packet_size().unwrap().min(PACKET_SIZE);
        let mut b = 0u8;
        loop {
            let data = vec![b; size];
            tokio::select! {
                result = ep_tx.send_async(data.into()) => {
                    match result {
                        Ok(()) => b = b.wrapping_add(1),
                        Err(e) => {
                            println!("device async send stopped: {e}");
                            break;
                        }
                    }
                }
                _ = stop_tx.notified() => break,
            }
        }
    });

    // Event loop: handle control requests.
    let mut ctrl_data = Vec::new();
    let mut stopped = false;
    while !stopped {
        if custom.wait_event().await.is_err() {
            break;
        }
        match custom.event() {
            Ok(event) => match event {
                Event::SetupHostToDevice(req) => {
                    if req.ctrl_req().request == req::STOP {
                        stopped = true;
                    }
                    ctrl_data = req.recv_all().unwrap();
                }
                Event::SetupDeviceToHost(req) => {
                    if req.ctrl_req().request == req::ECHO {
                        req.send(&ctrl_data).unwrap();
                    }
                }
                _ => {}
            },
            Err(e) => {
                if !stopped {
                    panic!("device async event error: {e}");
                }
                break;
            }
        }
    }

    stop.notify_waiters();
    let _ = rx_task.await;
    let _ = tx_task.await;
}

#[cfg(feature = "tokio")]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn transfer_async() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (vid, pid) = ids::TRANSFER;
    let (reg, custom, ep_rx, ep_tx) = setup_gadget_with_id(vid, pid, "transfer test device", "transfer-test-001");

    let host = tokio::task::spawn_blocking(run_host_pipelined);
    run_device_async(custom, ep_rx, ep_tx).await;
    host.await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;
    reg.remove().unwrap();
}
