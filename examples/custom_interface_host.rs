//! Host-side example for USB gadget with custom interface.

use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

use nusb::{
    transfer::{Bulk, ControlIn, ControlOut, ControlType, Direction, In, Out, Recipient},
    MaybeFuture,
};

fn main() {
    let device_info = nusb::list_devices()
        .wait()
        .expect("failed to list USB devices")
        .find(|d| d.vendor_id() == 6 && d.product_id() == 0x11)
        .expect("USB device not found");

    let device = device_info.open().wait().expect("failed to open device");
    println!("device opened: {device:?}");

    let cfg = device.active_configuration().expect("no active configuration");

    let mut my_if = None;
    let mut ep_in_addr = None;
    let mut ep_out_addr = None;

    for desc in cfg.interface_alt_settings() {
        println!("Interface {}:", desc.interface_number());
        println!("      Descriptor {:?}", desc);

        for ep in desc.endpoints() {
            println!("        Endpoint {ep:?}");
            println!("           Direction:        {:?}", ep.direction());
            println!("           Transfer type:    {:?}", ep.transfer_type());

            match ep.direction() {
                Direction::In => ep_in_addr = Some(ep.address()),
                Direction::Out => ep_out_addr = Some(ep.address()),
            }
            my_if = Some(desc.interface_number());
        }
        println!();
    }

    let my_if = my_if.unwrap();
    let ep_in_addr = ep_in_addr.unwrap();
    let ep_out_addr = ep_out_addr.unwrap();

    println!("claiming interface {my_if}");
    let intf = device.claim_interface(my_if).wait().expect("cannot claim interface");

    //device.reset().wait().expect("reset failed");

    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: 100,
            value: 200,
            index: my_if.into(),
            data: &[],
        },
        Duration::from_secs(1),
    )
    .wait()
    .expect("control error");

    let buf = [1, 2, 3, 4, 5, 6];
    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: 123,
            value: 222,
            index: my_if.into(),
            data: &buf,
        },
        Duration::from_secs(1),
    )
    .wait()
    .expect("control error");

    let rbuf = intf
        .control_in(
            ControlIn {
                control_type: ControlType::Vendor,
                recipient: Recipient::Interface,
                request: 123,
                value: 222,
                index: my_if.into(),
                length: buf.len() as u16,
            },
            Duration::from_secs(1),
        )
        .wait()
        .expect("control error");
    assert_eq!(&buf, rbuf.as_slice());

    let ep_in = intf.endpoint::<Bulk, In>(ep_in_addr).expect("cannot open IN endpoint");
    let ep_out = intf.endpoint::<Bulk, Out>(ep_out_addr).expect("cannot open OUT endpoint");

    thread::scope(|t| {
        t.spawn(|| {
            let mut reader = ep_in.reader(4096);
            println!("reading from endpoint 0x{ep_in_addr:02x}");
            let mut b = 0u8;
            for _ in 0..1024 {
                let mut buf = vec![0; 512];
                let n = reader.read(&mut buf).expect("cannot read");
                buf.truncate(n);

                println!("Read {n} bytes: {:x?}", &buf);

                if !buf.iter().all(|x| *x == b) {
                    panic!("wrong data received");
                }
                b = b.wrapping_add(1);
            }
        });

        t.spawn(|| {
            let mut writer = ep_out.writer(4096);
            println!("writing to endpoint 0x{ep_out_addr:02x}");
            let mut b = 0u8;
            for _ in 0..1024 {
                writer.write_all(&vec![b; 512]).expect("cannot write");
                writer.flush().expect("cannot flush");
                b = b.wrapping_add(1);
            }
        });
    });

    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: 255,
            value: 255,
            index: my_if.into(),
            data: &[],
        },
        Duration::from_secs(1),
    )
    .wait()
    .expect("control error");
}
