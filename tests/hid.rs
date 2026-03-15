mod common;
use common::*;
use serial_test::serial;

use usb_gadget::function::hid::Hid;

const HID_SUBCLASS: u8 = 0;
const HID_PROTOCOL: u8 = 2;

#[test]
#[serial]
fn hid() {
    init();

    let mut builder = Hid::builder();
    builder.protocol = HID_PROTOCOL;
    builder.sub_class = HID_SUBCLASS;
    builder.report_len = 8;
    builder.report_desc = vec![
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01, 0x75,
        0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x03, 0x95, 0x05, 0x75, 0x01, 0x05, 0x08,
        0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x03, 0x95, 0x06, 0x75, 0x08, 0x15,
        0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xc0,
    ];
    let (hid, func) = builder.build();

    let reg = reg(func);

    let (major, minor) = hid.device().unwrap();
    println!("HID device {major}:{minor} at {}", hid.status().path().unwrap().display());
    let dev_path = hid.device_path().unwrap();
    println!("HID device path: {}", dev_path.display());
    assert!(dev_path.exists(), "HID device path {dev_path:?} does not exist");

    check_host(|_device, cfg| {
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 3);
        assert!(intf.is_some(), "no HID interface (class 3) found on host");
        let intf = intf.unwrap();
        assert_eq!(intf.subclass(), HID_SUBCLASS, "HID subclass mismatch");
        assert_eq!(intf.protocol(), HID_PROTOCOL, "HID protocol mismatch");
        println!(
            "HID interface {}: class={}, subclass={}, protocol={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
        );
    });

    unreg(reg).unwrap();
}
