//! USB bulk and control transfer tests.
//!
//! Sets up a custom USB gadget with one bulk IN and one bulk OUT endpoint,
//! then performs transfers from both device and host sides in separate threads.
//!
//! Tests synchronous (pipelined), synchronous (no-timeout), async device-side IO,
//! and bulk throughput benchmarking.

mod ctrl;
mod dmabuf;
mod pipelined;
mod sync;
mod throughput;

#[cfg(feature = "tokio")]
mod async_io;

use nusb::{
    transfer::{Bulk, Direction, In, Out},
    MaybeFuture,
};
pub use serial_test::serial;
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
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

/// Vendor request codes.
mod req {
    pub const ECHO: u8 = 1;
    pub const STOP: u8 = 255;
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
    let reg =
        Gadget::new(Class::vendor_specific(255, 0), Id::new(vid, pid), Strings::new("test", product, serial))
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
