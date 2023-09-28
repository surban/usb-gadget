use macaddr::MacAddr6;
use std::{
    env,
    io::{Result, Write},
    os::unix::prelude::FileTypeExt,
    sync::Once,
    time::Duration,
};
use tempfile::NamedTempFile;
use tokio::{
    sync::{Mutex, MutexGuard},
    time::sleep,
};
use tracing_subscriber::{prelude::*, EnvFilter};

use usb_gadget::{
    default_udc,
    function::{Handle, Hid, Msd, MsdLun, Net, NetClass, Other, Serial, SerialClass},
    registered, remove_all, udcs, unbind_all, Class, Config, Gadget, Id, RegGadget, Strings,
};

async fn exclusive() -> MutexGuard<'static, ()> {
    static LOG_INIT: Once = Once::new();
    LOG_INIT.call_once(|| {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(EnvFilter::from_default_env())
            .init();
    });

    static LOCK: Mutex<()> = Mutex::const_new(());
    let guard = LOCK.lock().await;

    for gadget in registered().await.expect("cannot query registered gadgets") {
        if let Some(udc) = gadget.udc().await.unwrap() {
            println!("Unbinding gadget {} from UDC {}", gadget.name().to_string_lossy(), udc.to_string_lossy());
            gadget.bind(None).await.expect("cannot unbind existing gadget");
            sleep(Duration::from_secs(1)).await;
        }
    }

    guard
}

async fn reg_func(func: Handle) -> RegGadget {
    let udc = default_udc().await.expect("cannot get UDC");

    let reg =
        Gadget::new(Class::new(1, 2, 3), Id::new(4, 5), Strings::new("manufacturer", "product", "serial_number"))
            .with_config(Config::new("config").with_function(func))
            .bind(&udc)
            .await
            .expect("cannot bind to UDC");

    assert!(reg.is_attached());
    assert_eq!(reg.udc().await.unwrap().unwrap(), udc.name());

    println!("registered USB gadget {} at {}", reg.name().to_string_lossy(), reg.path().display());

    sleep(Duration::from_secs(3)).await;

    reg
}

async fn unreg(mut reg: RegGadget) -> Result<bool> {
    if env::var_os("KEEP_GADGET").is_some() {
        reg.detach();
        Ok(false)
    } else {
        reg.remove().await?;
        sleep(Duration::from_secs(1)).await;
        Ok(true)
    }
}

async fn net(net_class: NetClass) {
    let _mutex = exclusive().await;

    let dev_addr = MacAddr6::new(0x66, 0xf9, 0x7d, 0xf2, 0x3e, 0x2a);
    let host_addr = MacAddr6::new(0x7e, 0x21, 0xb2, 0xcb, 0xd4, 0x51);

    let mut builder = Net::builder(net_class);
    builder.dev_addr = Some(dev_addr);
    builder.host_addr = Some(host_addr);
    builder.qmult = Some(10);
    let (net, func) = builder.build();

    let reg = reg_func(func).await;

    println!(
        "Net device {} function at {}",
        net.ifname().await.unwrap().to_string_lossy(),
        net.path().unwrap().display()
    );

    assert_eq!(net.dev_addr().await.unwrap(), dev_addr);
    assert_eq!(net.host_addr().await.unwrap(), host_addr);

    if unreg(reg).await.unwrap() {
        assert!(net.path().is_err());
        assert!(net.dev_addr().await.is_err());
    }
}

async fn serial(serial_class: SerialClass) {
    let _mutex = exclusive().await;

    let mut builder = Serial::builder(serial_class);
    builder.console = Some(false);
    let (serial, func) = builder.build();

    let reg = reg_func(func).await;
    let tty = serial.tty().await.unwrap();

    println!("Serial device {} function at {}", tty.display(), serial.path().unwrap().display());

    assert!(tty.metadata().unwrap().file_type().is_char_device());

    if unreg(reg).await.unwrap() {
        assert!(serial.path().is_err());
        assert!(serial.tty().await.is_err());
    }
}

#[tokio::test]
async fn query_udcs() {
    let _mutex = exclusive().await;

    let udcs = udcs().await.unwrap();
    println!("USB device controllers:\n{:#?}", &udcs);

    for udc in udcs {
        println!("Name: {}", udc.name().to_string_lossy());
        println!("OTG: {:?}", udc.is_otg().await.unwrap());
        println!("Peripheral: {:?}", udc.is_a_peripheral().await.unwrap());
        println!("Current speed: {:?}", udc.current_speed().await.unwrap());
        println!("Max speed: {:?}", udc.max_speed().await.unwrap());
        println!("State: {:?}", udc.state().await.unwrap());
        println!("Function: {:?}", udc.function().await.unwrap());
        println!();
    }
}

#[tokio::test]
async fn ecm() {
    net(NetClass::Ecm).await
}

#[tokio::test]
async fn ecm_subset() {
    net(NetClass::EcmSubset).await
}

#[tokio::test]
async fn eem() {
    net(NetClass::Eem).await
}

#[tokio::test]
async fn ncm() {
    net(NetClass::Ncm).await
}

#[tokio::test]
async fn rndis() {
    net(NetClass::Rndis).await
}

#[tokio::test]
async fn acm() {
    serial(SerialClass::Acm).await
}

#[tokio::test]
async fn generic_serial() {
    serial(SerialClass::Generic).await
}

#[tokio::test]
async fn other_ecm() {
    let _mutex = exclusive().await;

    let dev_addr = "66:f9:7d:f2:3e:2a";

    let mut builder = Other::builder("ecm").unwrap();
    builder.set("dev_addr", dev_addr).unwrap();
    let (other, func) = builder.build();

    let reg = reg_func(func).await;

    println!("Other device at {}", other.path().unwrap().display());

    let mut dev_addr2 = other.get("dev_addr").await.unwrap();
    dev_addr2.retain(|&c| c != 0);
    let dev_addr2 = String::from_utf8_lossy(&dev_addr2).trim().to_string();
    assert_eq!(dev_addr, dev_addr2);

    unreg(reg).await.unwrap();
}

#[tokio::test]
async fn hid() {
    let _mutex = exclusive().await;

    // Keyboard HID description
    let mut builder = Hid::builder();
    builder.protocol = 1;
    builder.sub_class = 1;
    builder.report_len = 8;
    builder.report_desc = vec![
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01, 0x75,
        0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x03, 0x95, 0x05, 0x75, 0x01, 0x05, 0x08,
        0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x03, 0x95, 0x06, 0x75, 0x08, 0x15,
        0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xc0,
    ];
    let (hid, func) = builder.build();

    let reg = reg_func(func).await;

    println!("HID device {:?} at {}", hid.device().await.unwrap(), hid.path().unwrap().display());

    unreg(reg).await.unwrap();
}

#[tokio::test]
async fn msd() {
    let _mutex = exclusive().await;

    let mut file1 = NamedTempFile::new().unwrap();
    file1.write_all(&vec![1; 1_048_576]).unwrap();
    let path1 = file1.into_temp_path();

    let mut file2 = NamedTempFile::new().unwrap();
    file2.write_all(&vec![2; 1_048_576]).unwrap();
    let path2 = file2.into_temp_path();

    let mut builder = Msd::builder();
    builder.add_lun(MsdLun::new(&path1).unwrap());
    let mut lun2 = MsdLun::new(&path2).unwrap();
    lun2.cdrom = true;
    builder.add_lun(lun2);
    let (msd, func) = builder.build();

    let reg = reg_func(func).await;

    println!("MSD device at {}", msd.path().unwrap().display());

    sleep(Duration::from_secs(1)).await;

    msd.force_eject(0).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    msd.force_eject(1).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    msd.set_file(1, Some(&path1)).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    if unreg(reg).await.unwrap() {
        path1.close().expect("cannot delete temp file");
        path2.close().expect("cannot delete temp file");
    }
}

#[tokio::test]
async fn registered_gadgets() {
    let _mutex = exclusive().await;

    let reg = registered().await.unwrap();
    for gadget in reg {
        println!("Gadget {gadget:?} at {}", gadget.path().display());
        println!("UDC: {:?}", gadget.udc().await.unwrap());
        println!();
    }
}

#[tokio::test]
async fn remove_all_gadgets() {
    let _mutex = exclusive().await;

    remove_all().await.unwrap();
}

#[tokio::test]
async fn unbind_all_gadgets() {
    let _mutex = exclusive().await;

    unbind_all().await.unwrap();
}
