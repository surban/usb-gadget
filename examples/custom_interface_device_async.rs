//! Device-side example for USB gadget with custom interface using async IO.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use usb_gadget::{
    default_udc,
    function::custom::{Custom, Endpoint, EndpointDirection, Event, Interface},
    Class, Config, Gadget, Id, OsDescriptor, Strings, WebUsb,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

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

    let udc = default_udc().expect("cannot get UDC");
    let reg = Gadget::new(
        Class::new(255, 255, 3),
        Id::new(6, 0x11),
        Strings::new("manufacturer", "custom USB interface", "serial_number"),
    )
    .with_config(Config::new("config").with_function(handle))
    .with_os_descriptor(OsDescriptor::microsoft())
    .with_web_usb(WebUsb::new(0xf1, "http://webusb.org"))
    .bind(&udc)
    .expect("cannot bind to UDC");

    println!("Custom function at {}", custom.status().path().unwrap().display());
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
    reg.remove().unwrap();
}
