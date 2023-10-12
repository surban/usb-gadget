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

    println!("Serial device {} function at {}", tty.display(), serial.status().path().unwrap().display());

    assert!(tty.metadata().unwrap().file_type().is_char_device());

    if unreg(reg).unwrap() {
        assert!(serial.status().path().is_none());
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

#[cfg(feature = "tokio")]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn serial_status() {
    use std::time::Duration;
    use tokio::time::sleep;
    use usb_gadget::function::util::State;

    init();
    let _mutex = exclusive();

    let mut builder = Serial::builder(SerialClass::Acm);
    builder.console = Some(false);
    let (serial, func) = builder.build();

    let status = serial.status();
    assert_eq!(status.state(), State::Unregistered);

    let task = tokio::spawn(async move { status.bound().await });
    sleep(Duration::from_secs(1)).await;

    let reg = reg(func);

    println!("waiting for bound");
    task.await.unwrap().unwrap();

    let status = serial.status();
    assert_eq!(status.state(), State::Bound);

    let status = serial.status();
    let task = tokio::spawn(async move { status.unbound().await });
    sleep(Duration::from_secs(1)).await;

    if unreg(reg).unwrap() {
        let status = serial.status();
        assert_eq!(status.state(), State::Removed);

        println!("waiting for unbound");
        task.await.unwrap();
    }
}
