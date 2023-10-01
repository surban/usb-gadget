use std::os::unix::prelude::FileTypeExt;

use usb_gadget::function::serial::{Serial, SerialClass};

mod common;
use common::*;

fn serial(serial_class: SerialClass) {
    init();
    let _mutex = exclusive();

    let mut builder = Serial::builder(serial_class);
    builder.console = Some(false);
    let (serial, func) = builder.build();

    let reg = reg(func);
    let tty = serial.tty().unwrap();

    println!("Serial device {} function at {}", tty.display(), serial.path().unwrap().display());

    assert!(tty.metadata().unwrap().file_type().is_char_device());

    if unreg(reg).unwrap() {
        assert!(serial.path().is_err());
        assert!(serial.tty().is_err());
    }
}

#[test]
fn acm() {
    serial(SerialClass::Acm)
}

#[test]
fn generic_serial() {
    serial(SerialClass::Generic)
}
