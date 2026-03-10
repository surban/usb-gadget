//! Common test functions.
#![allow(dead_code)]

use std::{
    env,
    io::Result,
    sync::{Mutex, MutexGuard, Once},
    thread::sleep,
    time::Duration,
};

use nusb::MaybeFuture;
use usb_gadget::{
    default_udc, function::Handle, registered, Class, Config, Gadget, Id, OsDescriptor, RegGadget, Strings,
    WebUsb,
};

pub const TEST_VID: u16 = 4;
pub const TEST_PID: u16 = 5;
pub const TEST_MANUFACTURER: &str = "manufacturer";
pub const TEST_PRODUCT: &str = "product";
pub const TEST_SERIAL: &str = "serial_number";

pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        env_logger::init();

        for gadget in registered().expect("cannot query registered gadgets") {
            println!("Removing gadget {}", gadget.name().to_string_lossy(),);
            gadget.remove().expect("cannot remove existing gadget");
        }
        sleep(Duration::from_secs(1));
    });
}

pub fn reg(func: Handle) -> RegGadget {
    let udc = default_udc().expect("cannot get UDC");

    let reg = Gadget::new(
        Class::new(1, 2, 3),
        Id::new(TEST_VID, TEST_PID),
        Strings::new(TEST_MANUFACTURER, TEST_PRODUCT, TEST_SERIAL),
    )
    .with_config(Config::new("config").with_function(func))
    .bind(&udc)
    .expect("cannot bind to UDC");

    assert!(reg.is_attached());
    assert_eq!(reg.udc().unwrap().unwrap(), udc.name());

    println!(
        "bound USB gadget {} at {} to {}",
        reg.name().to_string_lossy(),
        reg.path().display(),
        udc.name().to_string_lossy()
    );

    sleep(Duration::from_secs(3));

    reg
}

pub fn reg_no_bind(func: Handle) -> RegGadget {
    let reg = Gadget::new(
        Class::new(1, 2, 3),
        Id::new(TEST_VID, TEST_PID),
        Strings::new(TEST_MANUFACTURER, TEST_PRODUCT, TEST_SERIAL),
    )
    .with_config(Config::new("config").with_function(func))
    .register()
    .expect("cannot register gadget");

    assert!(reg.is_attached());
    assert_eq!(reg.udc().unwrap(), None);

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

    println!("bound USB gadget {} at {}", reg.name().to_string_lossy(), reg.path().display());

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

/// Returns `true` if host-side checks should be skipped.
///
/// Set the `SKIP_HOST` environment variable to skip host-side USB verification.
pub fn skip_host() -> bool {
    env::var_os("SKIP_HOST").is_some()
}

/// Find the test USB device on the host side by VID/PID.
///
/// Retries for up to 5 seconds to allow for enumeration delay.
pub fn find_device() -> nusb::DeviceInfo {
    find_device_with_id(TEST_VID, TEST_PID)
}

/// Find a USB device on the host side by VID/PID.
///
/// Retries for up to 5 seconds to allow for enumeration delay.
pub fn find_device_with_id(vid: u16, pid: u16) -> nusb::DeviceInfo {
    for _ in 0..10 {
        if let Ok(mut iter) = nusb::list_devices().wait() {
            if let Some(dev) = iter.find(|d| d.vendor_id() == vid && d.product_id() == pid) {
                return dev;
            }
        }
        sleep(Duration::from_millis(500));
    }
    panic!("USB device with VID={vid:#06x} PID={pid:#06x} not found on host");
}

/// Find the test USB device, verify its descriptor strings, open it and
/// return the active configuration.
///
/// This checks VID, PID, manufacturer, product and serial number, then
/// opens the device and returns the [`nusb::Device`] together with its
/// active [`nusb::descriptors::ConfigurationDescriptor`].
pub fn open_device_on_host() -> nusb::Device {
    let dev_info = find_device();
    println!("found device on host: {dev_info:?}");

    assert_eq!(dev_info.vendor_id(), TEST_VID);
    assert_eq!(dev_info.product_id(), TEST_PID);
    assert_eq!(dev_info.manufacturer_string(), Some(TEST_MANUFACTURER));
    assert_eq!(dev_info.product_string(), Some(TEST_PRODUCT));
    assert_eq!(dev_info.serial_number(), Some(TEST_SERIAL));

    dev_info.open().wait().expect("cannot open device on host")
}

/// Run host-side verification unless `SKIP_HOST` is set.
///
/// Opens the test device, obtains the active configuration, and passes it
/// to the provided closure for function-specific checks.
pub fn check_host(f: impl FnOnce(&nusb::Device, nusb::descriptors::ConfigurationDescriptor<'_>)) {
    if skip_host() {
        return;
    }
    let device = open_device_on_host();
    let cfg = device.active_configuration().expect("no active configuration");
    f(&device, cfg);
}
