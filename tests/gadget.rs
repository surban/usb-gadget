mod common;
use common::*;
use serial_test::serial;

#[test]
#[serial]
fn registered_gadgets() {
    init();

    let reg = usb_gadget::registered().unwrap();
    for gadget in reg {
        println!("Gadget {gadget:?} at {}", gadget.path().display());
        println!("UDC: {:?}", gadget.udc().unwrap());
        println!();
    }
}

#[test]
#[serial]
fn remove_all_gadgets() {
    init();

    usb_gadget::remove_all().unwrap();
}

#[test]
#[serial]
fn unbind_all_gadgets() {
    init();

    usb_gadget::unbind_all().unwrap();
}
