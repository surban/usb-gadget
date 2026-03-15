mod common;
use common::*;
use serial_test::serial;

use usb_gadget::function::loopback::Loopback;

#[test]
#[serial]
fn loopback() {
    init();

    let (loopback, func) = Loopback::new();
    let reg = reg(func);

    println!("Loopback device at {}", loopback.status().path().unwrap().display());

    check_host(|_device, cfg| {
        // Loopback uses vendor-specific class 0xff.
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 0xff);
        assert!(intf.is_some(), "no vendor-specific interface (class 0xff) found on host");
        let intf = intf.unwrap();
        println!(
            "Loopback interface {}: class={}, subclass={}, protocol={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
        );

        // Loopback should have 2 bulk endpoints (IN + OUT).
        let num_endpoints = intf.num_endpoints();
        assert!(num_endpoints >= 2, "expected at least 2 endpoints, found {num_endpoints}");
    });

    unreg(reg).unwrap();
}

#[test]
#[serial]
fn loopback_builder() {
    init();

    let mut builder = Loopback::builder();
    builder.qlen = Some(32);
    builder.bulk_buflen = Some(4096);
    let (loopback, func) = builder.build();

    let reg = reg(func);

    println!("Loopback (builder) device at {}", loopback.status().path().unwrap().display());

    check_host(|_device, cfg| {
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 0xff);
        assert!(intf.is_some(), "no vendor-specific interface found on host");
    });

    unreg(reg).unwrap();
}
