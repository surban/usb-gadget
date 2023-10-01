//! Host-side tests for custom gadget.

use std::{thread, time::Duration};

use rusb::{open_device_with_vid_pid, request_type, Direction, RequestType};

#[test]
#[ignore = "host-side test"]
fn custom_host() {
    let mut hnd = open_device_with_vid_pid(4, 5).expect("USB device not found");
    let dev = hnd.device();
    println!("device opened: {hnd:?}");

    let cfg = dev.active_config_descriptor().unwrap();

    let mut my_if = None;
    let mut ep_in = None;
    let mut ep_out = None;

    for intf in cfg.interfaces() {
        println!("Interface {}:", intf.number());
        for desc in intf.descriptors() {
            println!("      Descriptor {:?}", desc);

            for ep in desc.endpoint_descriptors() {
                println!("        Endpoint {ep:?}");
                println!("           Direction:        {:?}", ep.direction());
                println!("           Transfer type:    {:?}", ep.transfer_type());

                match ep.direction() {
                    Direction::In => ep_in = Some(ep.address()),
                    Direction::Out => ep_out = Some(ep.address()),
                }
                my_if = Some(intf.number());
            }
        }
        println!();
    }

    let my_if = my_if.unwrap();
    let ep_in = ep_in.unwrap();
    let ep_out = ep_out.unwrap();

    println!("claiming interface {my_if}");
    hnd.claim_interface(my_if).expect("cannot claim interface");

    //hnd.reset().expect("reset failed");

    hnd.write_control(
        request_type(Direction::Out, RequestType::Vendor, rusb::Recipient::Interface),
        100,
        200,
        my_if.into(),
        &[],
        Duration::from_secs(1),
    )
    .expect("control error");

    let buf = [1, 2, 3, 4, 5, 6];
    hnd.write_control(
        request_type(Direction::Out, RequestType::Vendor, rusb::Recipient::Interface),
        123,
        222,
        my_if.into(),
        &buf,
        Duration::from_secs(1),
    )
    .expect("control error");

    let mut rbuf = vec![0; buf.len()];
    hnd.read_control(
        request_type(Direction::In, RequestType::Vendor, rusb::Recipient::Interface),
        123,
        222,
        my_if.into(),
        &mut rbuf,
        Duration::from_secs(1),
    )
    .expect("control error");
    assert_eq!(&buf, rbuf.as_slice());

    thread::scope(|t| {
        t.spawn(|| {
            println!("reading from endpoint {ep_in}");
            let mut b = 0;
            for _ in 0..1024 {
                let mut buf = vec![0; 512];
                let n = hnd.read_bulk(ep_in, &mut buf, Duration::from_secs(1)).expect("cannot read");
                buf.truncate(n);

                println!("Read {n} bytes: {:x?}", &buf);

                if !buf.iter().all(|x| *x == b) {
                    panic!("wrong data received");
                }
                b = b.wrapping_add(1);
            }
        });

        t.spawn(|| {
            println!("writing to endpoint {ep_out}");
            let mut b = 0u8;
            for _ in 0..1024 {
                hnd.write_bulk(ep_out, &vec![b; 512], Duration::from_secs(1)).expect("cannot write");
                b = b.wrapping_add(1);
            }
        });
    });

    hnd.write_control(
        request_type(Direction::Out, RequestType::Vendor, rusb::Recipient::Interface),
        255,
        255,
        my_if.into(),
        &[],
        Duration::from_secs(1),
    )
    .expect("control error");
}
