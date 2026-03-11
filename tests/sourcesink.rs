mod common;
use common::*;

use usb_gadget::function::sourcesink::SourceSink;

#[test]
fn sourcesink() {
    init();
    let _mutex = exclusive();

    let (ss, func) = SourceSink::new();
    let reg = reg(func);

    println!("SourceSink device at {}", ss.status().path().unwrap().display());

    check_host(|_device, cfg| {
        // SourceSink uses vendor-specific class 0xff.
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 0xff);
        assert!(intf.is_some(), "no vendor-specific interface (class 0xff) found on host");
        let intf = intf.unwrap();
        println!(
            "SourceSink interface {}: class={}, subclass={}, protocol={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
        );

        // SourceSink should have at least 2 bulk endpoints (IN + OUT).
        let num_endpoints = intf.num_endpoints();
        assert!(num_endpoints >= 2, "expected at least 2 endpoints, found {num_endpoints}");
    });

    unreg(reg).unwrap();
}

#[test]
fn sourcesink_builder() {
    init();
    let _mutex = exclusive();

    let mut builder = SourceSink::builder();
    builder.pattern = Some(1);
    builder.bulk_buflen = Some(4096);
    builder.bulk_qlen = Some(32);
    let (ss, func) = builder.build();

    let reg = reg(func);

    println!("SourceSink (builder) device at {}", ss.status().path().unwrap().display());

    check_host(|_device, cfg| {
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 0xff);
        assert!(intf.is_some(), "no vendor-specific interface found on host");
    });

    unreg(reg).unwrap();
}
