mod common;
use common::*;
use serial_test::serial;
use std::io::ErrorKind;

use usb_gadget::{function::other::Other, registered, Class, Config, Gadget, Id, Strings};

#[test]
#[serial]
fn other_ecm() {
    init();

    let dev_addr = "66:f9:7d:f2:3e:2a";

    let mut builder = Other::builder("ecm").unwrap();
    builder.set("dev_addr", dev_addr).unwrap();
    let (other, func) = builder.build();

    let reg = reg(func);

    println!("Other device at {}", other.status().path().unwrap().display());

    let mut dev_addr2 = other.get("dev_addr").unwrap();
    dev_addr2.retain(|&c| c != 0);
    let dev_addr2 = String::from_utf8_lossy(&dev_addr2).trim().to_string();
    assert_eq!(dev_addr, dev_addr2);

    check_host(|_device, cfg| {
        // ECM is CDC Ethernet: class 2 (Communications), subclass 6 (Ethernet Networking).
        let ecm_intf = cfg.interface_alt_settings().find(|desc| desc.class() == 2 && desc.subclass() == 6);
        assert!(ecm_intf.is_some(), "no CDC ECM interface (class 2, subclass 6) found on host");
        let ecm_intf = ecm_intf.unwrap();
        println!(
            "ECM interface {}: class={}, subclass={}, protocol={}",
            ecm_intf.interface_number(),
            ecm_intf.class(),
            ecm_intf.subclass(),
            ecm_intf.protocol(),
        );
    });

    unreg(reg).unwrap();
}

#[test]
#[serial]
fn other_unsupported_property() {
    init();

    let mut builder = Other::builder("ecm").unwrap();
    builder.set("no_such_property", "1").unwrap();
    let (_other, func) = builder.build();

    let res = Gadget::new(
        Class::new(1, 2, 3),
        Id::new(TEST_VID, TEST_PID),
        Strings::new(TEST_MANUFACTURER, TEST_PRODUCT, TEST_SERIAL),
    )
    .with_config(Config::new("config").with_function(func))
    .register();

    let err = res.expect_err("registration with unsupported property must fail");
    println!("registration failed: {err}");
    assert_eq!(err.kind(), ErrorKind::Unsupported);

    assert!(registered().unwrap().is_empty(), "partially registered gadget was not removed");
}

#[test]
#[serial]
fn other_unsupported_property_keep_on_error() {
    init();

    let mut builder = Other::builder("ecm").unwrap();
    builder.set("no_such_property", "1").unwrap();
    let (_other, func) = builder.build();

    let mut gadget = Gadget::new(
        Class::new(1, 2, 3),
        Id::new(TEST_VID, TEST_PID),
        Strings::new(TEST_MANUFACTURER, TEST_PRODUCT, TEST_SERIAL),
    )
    .with_config(Config::new("config").with_function(func));
    gadget.keep_on_error = true;

    let err = gadget.register().expect_err("registration with unsupported property must fail");
    assert_eq!(err.kind(), ErrorKind::Unsupported);

    let gadgets = registered().unwrap();
    assert_eq!(gadgets.len(), 1, "partially registered gadget was not kept");
    for gadget in gadgets {
        let functions = gadget.functions().unwrap();
        println!("kept gadget {} with functions {functions:?}", gadget.path().display());
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].driver(), "ecm");
        gadget.remove().unwrap();
    }
}
