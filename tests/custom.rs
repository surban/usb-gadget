use std::{
    io::ErrorKind,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use uuid::uuid;

use usb_gadget::{
    function::custom::{Custom, Endpoint, EndpointDirection, Event, Interface, OsExtCompat, OsExtProp},
    Class,
};

mod common;
use common::*;

#[test]
fn custom() {
    init();
    let _mutex = exclusive();

    let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let (mut custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        )
        .build();

    let reg = reg(handle);
    println!("Custom function at {}", custom.status().path().unwrap().display());
    println!("real interface address 0: {}", custom.real_address(0).unwrap());
    println!();

    let ep1_control = ep1_rx.control().unwrap();
    println!("ep1 unclaimed: {:?}", ep1_control.unclaimed_fifo());
    println!("ep1 real address: {}", ep1_control.real_address().unwrap());
    println!("ep1 descriptor: {:?}", ep1_control.descriptor().unwrap());
    println!();

    let ep2_control = ep2_tx.control().unwrap();
    println!("ep2 unclaimed: {:?}", ep2_control.unclaimed_fifo());
    println!("ep2 real address: {}", ep2_control.real_address().unwrap());
    println!("ep2 descriptor: {:?}", ep2_control.descriptor().unwrap());
    println!();

    thread::sleep(Duration::from_secs(1));

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().path().is_none());
    }
}

#[test]
fn custom_with_os_desc() {
    init();
    let _mutex = exclusive();

    let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let (mut custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir))
                .with_os_ext_compat(OsExtCompat::winusb())
                .with_os_ext_prop(OsExtProp::device_interface_guid(uuid!("8FE6D4D7-49DD-41E7-9486-49AFC6BFE475")))
                .with_os_ext_prop(OsExtProp::device_idle_enabled(true))
                .with_os_ext_prop(OsExtProp::default_idle_state(true))
                .with_os_ext_prop(OsExtProp::default_idle_timeout(5000))
                .with_os_ext_prop(OsExtProp::user_set_device_idle_enabled(true))
                .with_os_ext_prop(OsExtProp::system_wake_enabled(false)),
        )
        .build();

    let reg = reg_with_os_desc(handle);
    println!("Custom function at {}", custom.status().path().unwrap().display());
    println!("real interface address 0: {}", custom.real_address(0).unwrap());
    println!();

    let ep1_control = ep1_rx.control().unwrap();
    println!("ep1 unclaimed: {:?}", ep1_control.unclaimed_fifo());
    println!("ep1 real address: {}", ep1_control.real_address().unwrap());
    println!("ep1 descriptor: {:?}", ep1_control.descriptor().unwrap());
    println!();

    let ep2_control = ep2_tx.control().unwrap();
    println!("ep2 unclaimed: {:?}", ep2_control.unclaimed_fifo());
    println!("ep2 real address: {}", ep2_control.real_address().unwrap());
    println!("ep2 descriptor: {:?}", ep2_control.descriptor().unwrap());
    println!();

    thread::sleep(Duration::from_secs(10));

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().path().is_none());
    }
}

#[test]
#[ignore = "host-side support required"]
fn custom_with_host() {
    init();
    let _mutex = exclusive();

    usb_gadget::remove_all().expect("cannot remove all gadgets");

    let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let (mut custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 2), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        )
        .build();

    let reg = reg(handle);
    println!("Custom function at {}", custom.status().path().unwrap().display());
    println!("real interface address 0: {}", custom.real_address(0).unwrap());
    println!();

    let ep1_control = ep1_rx.control().unwrap();
    println!("ep1 unclaimed: {:?}", ep1_control.unclaimed_fifo());
    println!("ep1 real address: {}", ep1_control.real_address().unwrap());
    println!("ep1 descriptor: {:?}", ep1_control.descriptor().unwrap());
    println!();

    let ep2_control = ep2_tx.control().unwrap();
    println!("ep2 unclaimed: {:?}", ep2_control.unclaimed_fifo());
    println!("ep2 real address: {}", ep2_control.real_address().unwrap());
    println!("ep2 descriptor: {:?}", ep2_control.descriptor().unwrap());
    println!();

    let stop = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        s.spawn(|| {
            let size = ep1_rx.max_packet_size().unwrap();
            let mut b = 0;
            while !stop.load(Ordering::Relaxed) {
                let data = ep1_rx.recv_timeout(size, Duration::from_secs(1)).expect("recv failed");
                match data {
                    Some(data) => {
                        println!("received {} bytes: {data:x?}", data.len());
                        if !data.iter().all(|x| *x == b) {
                            panic!("wrong data received");
                        }
                        b = b.wrapping_add(1);
                    }
                    None => {
                        println!("receive empty");
                    }
                }
            }
        });

        s.spawn(|| {
            let size = ep2_tx.max_packet_size().unwrap();
            let mut b = 0u8;
            while !stop.load(Ordering::Relaxed) {
                let data = vec![b; size];
                match ep2_tx.send_timeout(data, Duration::from_secs(1)) {
                    Ok(()) => {
                        println!("sent data {b} of size {size} bytes");
                        b = b.wrapping_add(1);
                    }
                    Err(err) if err.kind() == ErrorKind::TimedOut => println!("send timeout"),
                    Err(err) => panic!("send failed: {err}"),
                }
            }
        });

        s.spawn(|| {
            let mut ctrl_data = Vec::new();

            while !stop.load(Ordering::Relaxed) {
                if let Some(event) = custom.event_timeout(Duration::from_secs(1)).expect("event failed") {
                    println!("Event: {event:?}");
                    match event {
                        Event::SetupHostToDevice(req) => {
                            if req.ctrl_req().request == 255 {
                                println!("Stopping");
                                stop.store(true, Ordering::Relaxed);
                            }
                            ctrl_data = req.recv_all().unwrap();
                            println!("Control data: {ctrl_data:x?}");
                        }
                        Event::SetupDeviceToHost(req) => {
                            println!("Replying with data");
                            req.send(&ctrl_data).unwrap();
                        }
                        _ => (),
                    }
                } else {
                    println!("no event");
                }
            }
        });
    });

    thread::sleep(Duration::from_secs(1));

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().path().is_none());
    }
}

#[cfg(feature = "tokio")]
#[tokio::test]
#[ignore = "host-side support required"]
#[allow(clippy::await_holding_lock)]
async fn async_custom_with_host() {
    init();
    let _mutex = exclusive();

    usb_gadget::remove_all().expect("cannot remove all gadgets");

    let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let (mut custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 2), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        )
        .build();

    let reg = reg(handle);
    println!("Custom function at {}", custom.status().path().unwrap().display());
    println!("real interface address 0: {}", custom.real_address(0).unwrap());
    println!();

    let ep1_control = ep1_rx.control().unwrap();
    println!("ep1 unclaimed: {:?}", ep1_control.unclaimed_fifo());
    println!("ep1 real address: {}", ep1_control.real_address().unwrap());
    println!("ep1 descriptor: {:?}", ep1_control.descriptor().unwrap());
    println!();

    let ep2_control = ep2_tx.control().unwrap();
    println!("ep2 unclaimed: {:?}", ep2_control.unclaimed_fifo());
    println!("ep2 real address: {}", ep2_control.real_address().unwrap());
    println!("ep2 descriptor: {:?}", ep2_control.descriptor().unwrap());
    println!();

    let stop = Arc::new(AtomicBool::new(false));

    let stop1 = stop.clone();
    tokio::spawn(async move {
        let size = ep1_rx.max_packet_size().unwrap();
        let mut b = 0;
        while !stop1.load(Ordering::Relaxed) {
            let data = ep1_rx.recv_async(size).await.expect("recv_async failed");
            match data {
                Some(data) => {
                    println!("received {} bytes: {data:x?}", data.len());
                    if !data.iter().all(|x| *x == b) {
                        panic!("wrong data received");
                    }
                    b = b.wrapping_add(1);
                }
                None => {
                    println!("receive empty");
                }
            }
        }
    });

    let stop2 = stop.clone();
    tokio::spawn(async move {
        let size = ep2_tx.max_packet_size().unwrap();
        let mut b = 0u8;
        while !stop2.load(Ordering::Relaxed) {
            let data = vec![b; size];
            match ep2_tx.send_async(data).await {
                Ok(()) => {
                    println!("sent data {b} of size {size} bytes");
                    b = b.wrapping_add(1);
                }
                Err(err) => panic!("send failed: {err}"),
            }
        }
    });

    let mut ctrl_data = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        custom.wait_event().await.expect("wait for event failed");
        println!("event ready");
        let event = custom.event().expect("event failed");

        println!("Event: {event:?}");
        match event {
            Event::SetupHostToDevice(req) => {
                if req.ctrl_req().request == 255 {
                    println!("Stopping");
                    stop.store(true, Ordering::Relaxed);
                }
                ctrl_data = req.recv_all().unwrap();
                println!("Control data: {ctrl_data:x?}");
            }
            Event::SetupDeviceToHost(req) => {
                println!("Replying with data");
                req.send(&ctrl_data).unwrap();
            }
            _ => (),
        }
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().path().is_none());
    }
}
