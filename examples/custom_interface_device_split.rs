//! Device-side example for USB gadget with custom interface.
//! This example follows the custom_interface_device.rs, but also
//! demonstrates how it is possible to run the privileged parts of
//! gadget setup (interacting with ConfigFS) in a different process to
//! the custom function parts (FunctionFS). This example runs
//! the main gadget logic in an unprivileged process.

use bytes::BytesMut;
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
    function::custom::{Custom, Endpoint, EndpointDirection, Event, Interface},
    Class, Config, Gadget, Id, OsDescriptor, Strings, WebUsb,
};
use usb_gadget::function::custom::{EndpointReceiver, EndpointSender};

fn main() {
    env_logger::init();

    let existing = std::env::var("EXISTING_FFS").ok();
    let register_only = std::env::var("REGISTER_ONLY").ok().is_some();

    let (ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let mut builder = Custom::builder();

    if register_only {
        // We are only registering and binding the gadget, and leaving the FunctionFS interactions
        // to another process.
        builder.ffs_no_init = true;
        builder.ffs_uid = Some(std::env::var("SUDO_UID").unwrap().parse().unwrap());
        builder.ffs_gid = Some(std::env::var("SUDO_GID").unwrap().parse().unwrap());
    } else {
        builder = builder.with_interface(
            Interface::new(Class::vendor_specific(1, 2), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        );
    }

    let (reg, custom) = if let Some(ref path) = existing {
        (None, builder.existing(path).unwrap())
    } else {
        let (mut custom, handle) = builder.build();

        usb_gadget::remove_all().expect("cannot remove all gadgets");

        let udc = default_udc().expect("cannot get UDC");
        let gadget = Gadget::new(
            Class::new(255, 255, 3),
            Id::new(6, 0x11),
            Strings::new("manufacturer", "custom USB interface", "serial_number"),
        )
            .with_config(Config::new("config").with_function(handle))
            .with_os_descriptor(OsDescriptor::microsoft())
            .with_web_usb(WebUsb::new(0xf1, "http://webusb.org"));

        let reg = gadget
            .register()
            .expect("cannot register gadget");

        if register_only {
            let ffs_dir = custom.ffs_dir().unwrap();
            println!("FunctionFS dir mounted at {}", ffs_dir.display());
            println!("You can now run this program again as unprivileged user:");
            println!("EXISTING_FFS={} {}", ffs_dir.display(), std::env::args().next().unwrap());

            let mut ep1_path = ffs_dir.clone();
            ep1_path.push("ep1");
            while std::fs::metadata(&ep1_path).is_err() {
                thread::sleep(Duration::from_secs(1));
            }

            println!("Detected ep1 in FunctionFS dir, this means descriptors have been written to ep0.");
            println!("Now binding gadget to UDC (making it active)...");
        }

        reg.bind(Some(&udc)).expect("cannot bind to UDC");

        println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
        println!();

        (Some(reg), custom)
    };

    if register_only {
        println!("Waiting for the gadget to become unbound. If you stop the other process, this will happen automatically.");
        while reg.as_ref().unwrap().udc().unwrap().is_some() {
            thread::sleep(Duration::from_secs(1));
        }
    } else {
        if existing.is_some() {
            println!("The FunctionFS setup is done, you can type 'yes' in the other process and hit <ENTER>");
        }
        run(ep1_rx, ep2_tx, custom);
    }

    if let Some(reg) = reg {
        println!("Unregistering");
        reg.remove().unwrap();
    }
}

fn run(mut ep1_rx: EndpointReceiver, mut ep2_tx: EndpointSender, mut custom: Custom) {
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
                let data = ep1_rx
                    .recv_timeout(BytesMut::with_capacity(size), Duration::from_secs(1))
                    .expect("recv failed");
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
                match ep2_tx.send_timeout(data.into(), Duration::from_secs(1)) {
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
}
