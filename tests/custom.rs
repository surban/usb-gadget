use std::{thread, time::Duration};
use uuid::uuid;

use usb_gadget::{
    function::custom::{Custom, Endpoint, EndpointDirection, Interface, OsExtCompat, OsExtProp},
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

    let (custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        )
        .build();

    let reg = reg(handle);
    println!("Custom function at {}", custom.status().path().unwrap().display());
    println!();

    println!("Getting ep1_rx control");
    let _ep1_control = ep1_rx.control().unwrap();

    println!("Getting ep2_tx control");
    let _ep2_control = ep2_tx.control().unwrap();

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
                .with_os_ext_prop(OsExtProp::device_interface_guid(uuid!(
                    "8FE6D4D7-49DD-41E7-9486-49AFC6BFE475"
                ))),
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
