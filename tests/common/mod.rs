//! Common test functions.
#![allow(dead_code)]

use std::{
    env,
    io::Result,
    sync::{Mutex, MutexGuard, Once},
    thread::sleep,
    time::Duration,
};

use usb_gadget::{
    default_udc, function::Handle, registered, Class, Config, Gadget, Id, OsDescriptor, RegGadget, Strings,
    WebUsb,
};

pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        env_logger::init();

        for gadget in registered().expect("cannot query registered gadgets") {
            if let Some(udc) = gadget.udc().unwrap() {
                println!(
                    "Unbinding gadget {} from UDC {}",
                    gadget.name().to_string_lossy(),
                    udc.to_string_lossy()
                );
                gadget.bind(None).expect("cannot unbind existing gadget");
                sleep(Duration::from_secs(1));
            }
        }
    });
}

pub fn reg(func: Handle) -> RegGadget {
    let udc = default_udc().expect("cannot get UDC");

    let reg =
        Gadget::new(Class::new(1, 2, 3), Id::new(4, 5), Strings::new("manufacturer", "product", "serial_number"))
            .with_config(Config::new("config").with_function(func))
            .bind(&udc)
            .expect("cannot bind to UDC");

    assert!(reg.is_attached());
    assert_eq!(reg.udc().unwrap().unwrap(), udc.name());

    println!("registered USB gadget {} at {}", reg.name().to_string_lossy(), reg.path().display());

    sleep(Duration::from_secs(3));

    reg
}

pub fn reg_with_os_desc(func: Handle) -> RegGadget {
    let udc = default_udc().expect("cannot get UDC");

    let reg = Gadget::new(
        Class::new(255, 255, 3),
        Id::new(6, 0x11),
        Strings::new("manufacturer", "product with OS descriptor", "serial_number"),
    )
    .with_config(Config::new("config").with_function(func))
    .with_os_descriptor(OsDescriptor::microsoft())
    .with_web_usb(WebUsb::new(0xf1, "http://webusb.org"))
    .bind(&udc)
    .expect("cannot bind to UDC");

    assert!(reg.is_attached());
    assert_eq!(reg.udc().unwrap().unwrap(), udc.name());

    println!("registered USB gadget {} at {}", reg.name().to_string_lossy(), reg.path().display());

    sleep(Duration::from_secs(3));

    reg
}

pub fn unreg(mut reg: RegGadget) -> Result<bool> {
    if env::var_os("KEEP_GADGET").is_some() {
        reg.detach();
        Ok(false)
    } else {
        reg.remove()?;
        sleep(Duration::from_secs(1));
        Ok(true)
    }
}

pub fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap()
}
