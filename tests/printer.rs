mod common;
use common::*;

use usb_gadget::function::printer::Printer;

#[test]
fn printer() {
    init();

    // Keyboard printer description
    let mut builder = Printer::builder();
    builder.pnp_string = Some("Rust Printer".to_string());
    builder.qlen = Some(20);
    let (printer, func) = builder.build();

    let reg = reg(func);

    println!("printer device at {}", printer.status().path().unwrap().display());

    // Host-side verification: check that the device is visible via USB.
    check_host(|_device, cfg| {
        // USB Printer class = 7, subclass = 1.
        let printer_intf = cfg.interface_alt_settings().find(|desc| desc.class() == 7 && desc.subclass() == 1);
        assert!(printer_intf.is_some(), "no printer interface (class 7, subclass 1) found on host");

        let printer_intf = printer_intf.unwrap();
        println!(
            "printer interface {}: class={}, subclass={}, protocol={}",
            printer_intf.interface_number(),
            printer_intf.class(),
            printer_intf.subclass(),
            printer_intf.protocol(),
        );
    });

    unreg(reg).unwrap();
}
