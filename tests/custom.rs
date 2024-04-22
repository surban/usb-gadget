use std::{thread, time::Duration};
use uuid::uuid;

use usb_gadget::{
    default_udc,
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
    println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
    println!();

    println!("Getting ep1_rx control");
    let _ep1_control = ep1_rx.control().unwrap();

    println!("Getting ep2_tx control");
    let _ep2_control = ep2_tx.control().unwrap();

    thread::sleep(Duration::from_secs(1));

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().unwrap().path().is_none());
    }
}

#[test]
#[ignore = "test requires a USB connection to a USB host"]
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
    println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
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
        assert!(custom.status().unwrap().path().is_none());
    }
}

#[test]
fn custom_no_disconnect() {
    init();
    let _mutex = exclusive();

    let (reg, ffs_dir) = {
        let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
        let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

        let mut builder = Custom::builder().with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        );
        builder.ffs_no_disconnect = true;
        builder.ffs_file_mode = Some(0o770);
        builder.ffs_root_mode = Some(0o777);

        let (mut custom, handle) = builder.build();

        let reg = reg(handle);
        println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
        println!();

        let ffs_dir = custom.ffs_dir().unwrap();
        println!("FunctionFS is at {}", ffs_dir.display());

        println!("Getting ep1_rx control");
        let _ep1_control = ep1_rx.control().unwrap();

        println!("Getting ep2_tx control");
        let _ep2_control = ep2_tx.control().unwrap();

        thread::sleep(Duration::from_secs(3));

        println!("Dropping custom interface");
        (reg, ffs_dir)
    };

    println!("Dropped custom interface");
    thread::sleep(Duration::from_secs(3));

    {
        println!("Recreating custom interface using existing FunctionFS mount");

        let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
        let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();
        let mut custom = Custom::builder()
            .with_interface(
                Interface::new(Class::vendor_specific(1, 1), "custom interface")
                    .with_endpoint(Endpoint::bulk(ep1_dir))
                    .with_endpoint(Endpoint::bulk(ep2_dir)),
            )
            .existing(&ffs_dir)
            .unwrap();

        assert_eq!(ffs_dir, custom.ffs_dir().unwrap());

        println!("Getting ep1_rx control");
        let _ep1_control = ep1_rx.control().unwrap();

        println!("Getting ep2_tx control");
        let _ep2_control = ep2_tx.control().unwrap();

        println!("Reactivating USB gadget");
        reg.bind(Some(&default_udc().unwrap())).unwrap();

        thread::sleep(Duration::from_secs(3));
        println!("Dropping custom interface");
    }

    println!("Unregistering");
    unreg(reg).unwrap();
}

#[test]
fn custom_ext_init() {
    init();
    let _mutex = exclusive();

    let (reg, ffs_dir) = {
        let mut builder = Custom::builder();
        builder.ffs_no_init = true;

        let (mut custom, handle) = builder.build();

        let reg = reg_no_bind(handle);
        println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
        println!();

        let ffs_dir = custom.ffs_dir().unwrap();
        println!("FunctionFS is at {}", ffs_dir.display());
        custom.fd().expect_err("fd must not be available");

        thread::sleep(Duration::from_secs(3));

        println!("Dropping custom interface");
        (reg, ffs_dir)
    };

    println!("Dropped custom interface");
    thread::sleep(Duration::from_secs(3));

    {
        println!("Creating custom interface using existing FunctionFS mount");

        let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
        let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();
        let mut custom = Custom::builder()
            .with_interface(
                Interface::new(Class::vendor_specific(1, 1), "custom interface")
                    .with_endpoint(Endpoint::bulk(ep1_dir))
                    .with_endpoint(Endpoint::bulk(ep2_dir)),
            )
            .existing(&ffs_dir)
            .unwrap();

        assert_eq!(ffs_dir, custom.ffs_dir().unwrap());
        assert!(custom.status().is_none());

        println!("Getting ep1_rx control");
        let _ep1_control = ep1_rx.control().unwrap();

        println!("Getting ep2_tx control");
        let _ep2_control = ep2_tx.control().unwrap();

        println!("Activating USB gadget");
        reg.bind(Some(&default_udc().unwrap())).unwrap();

        thread::sleep(Duration::from_secs(3));
        println!("Dropping custom interface");
    }

    println!("Unregistering");
    unreg(reg).unwrap();
}
