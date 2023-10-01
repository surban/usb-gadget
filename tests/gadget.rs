mod common;
use common::*;

#[test]
fn registered_gadgets() {
    init();
    let _mutex = exclusive();

    let reg = usb_gadget::registered().unwrap();
    for gadget in reg {
        println!("Gadget {gadget:?} at {}", gadget.path().display());
        println!("UDC: {:?}", gadget.udc().unwrap());
        println!();
    }
}

#[test]
fn remove_all_gadgets() {
    init();
    let _mutex = exclusive();

    usb_gadget::remove_all().unwrap();
}

#[test]
fn unbind_all_gadgets() {
    init();
    let _mutex = exclusive();

    usb_gadget::unbind_all().unwrap();
}
