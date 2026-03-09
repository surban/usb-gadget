mod common;
use common::*;

use usb_gadget::function::other::Other;

#[test]
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
