//! USB bulk and control transfer tests.
//!
//! Sets up a custom USB gadget with one bulk IN and one bulk OUT endpoint,
//! then performs transfers from both device and host sides in separate threads.
//!
//! Tests both synchronous and async device-side IO.

use bytes::BytesMut;
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

const VID: u16 = 0x1234;
const PID: u16 = 0x0020;
const ROUNDS: usize = 64;
const PACKET_SIZE: usize = 512;

/// Vendor request codes.
mod req {
    pub const ECHO: u8 = 1;
    pub const STOP: u8 = 255;
}

/// Sets up the gadget and returns the registration, custom handle, receiver, and sender.
fn setup_gadget() -> (
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
    let reg = Gadget::new(
        Class::new(255, 255, 0),
        Id::new(VID, PID),
        Strings::new("test", "transfer test device", "transfer-test-001"),
    )
    .with_config(Config::new("config").with_function(handle))
    .bind(&udc)
    .expect("cannot bind to UDC");

    (reg, custom, ep_rx, ep_tx)
}

/// Run the device side: receive data, send data, and handle control requests.
fn run_device(
    mut custom: Custom, mut ep_rx: usb_gadget::function::custom::EndpointReceiver,
    mut ep_tx: usb_gadget::function::custom::EndpointSender,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let stop3 = stop.clone();

    thread::scope(|s| {
        // Receive thread: read packets from host, verify content.
        s.spawn(move || {
            let size = ep_rx.max_packet_size().unwrap();
            let mut expected = 0u8;
            while !stop2.load(Ordering::Relaxed) {
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
                    Err(e) if stop2.load(Ordering::Relaxed) => {
                        println!("device recv stopped: {e}");
                        break;
                    }
                    Err(e) => panic!("device recv error: {e}"),
                }
            }
        });

        // Send thread: send packets to host.
        s.spawn(move || {
            let size = ep_tx.max_packet_size().unwrap().min(PACKET_SIZE);
            let mut b = 0u8;
            while !stop3.load(Ordering::Relaxed) {
                let data = vec![b; size];
                match ep_tx.send_timeout(data.into(), Duration::from_secs(2)) {
                    Ok(()) => {
                        b = b.wrapping_add(1);
                    }
                    Err(e) if e.kind() == ErrorKind::TimedOut => {}
                    Err(e) if stop3.load(Ordering::Relaxed) => {
                        println!("device send stopped: {e}");
                        break;
                    }
                    Err(e) => panic!("device send error: {e}"),
                }
            }
        });

        // Event thread: handle control requests.
        let mut ctrl_data = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            match custom.event_timeout(Duration::from_secs(1)) {
                Ok(Some(event)) => match event {
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
                Ok(None) => {}
                Err(e) => {
                    if !stop.load(Ordering::Relaxed) {
                        panic!("device event error: {e}");
                    }
                    break;
                }
            }
        }
    });
}

/// Run the host side: open device, perform control + bulk transfers.
fn run_host() {
    // Wait for device to enumerate.
    let dev_info = find_device_with_id(VID, PID);
    println!("host: found device: {dev_info:?}");

    let device = dev_info.open().wait().expect("host: cannot open device");
    let cfg = device.active_configuration().expect("host: no active configuration");

    // Find endpoints.
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

    // --- Bulk transfer test ---
    let ep_in = intf.endpoint::<Bulk, In>(ep_in_addr).expect("host: cannot open IN endpoint");
    let ep_out = intf.endpoint::<Bulk, Out>(ep_out_addr).expect("host: cannot open OUT endpoint");

    thread::scope(|t| {
        // Read from device.
        t.spawn(|| {
            let mut reader = ep_in.reader(4096);
            let mut expected = 0u8;
            for i in 0..ROUNDS {
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
            println!("host: read {ROUNDS} bulk packets OK");
        });

        // Write to device.
        t.spawn(|| {
            let mut writer = ep_out.writer(4096);
            let mut b = 0u8;
            for _ in 0..ROUNDS {
                writer.write_all(&vec![b; PACKET_SIZE]).expect("host: write failed");
                writer.flush().expect("host: flush failed");
                b = b.wrapping_add(1);
            }
            println!("host: wrote {ROUNDS} bulk packets OK");
        });
    });

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

    let (reg, custom, ep_rx, ep_tx) = setup_gadget();

    // Run device and host sides concurrently.
    thread::scope(|s| {
        s.spawn(|| run_device(custom, ep_rx, ep_tx));
        s.spawn(run_host);
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Run the device side using async IO.
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
                        Ok(()) => {
                            b = b.wrapping_add(1);
                        }
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

    let (reg, custom, ep_rx, ep_tx) = setup_gadget();

    // Run device async and host (in a blocking thread) concurrently.
    let host = tokio::task::spawn_blocking(run_host);
    run_device_async(custom, ep_rx, ep_tx).await;
    host.await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;
    reg.remove().unwrap();
}
