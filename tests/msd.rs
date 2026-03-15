mod common;
use common::*;
use serial_test::serial;

use std::{io::Write, thread::sleep, time::Duration};
use tempfile::NamedTempFile;

use usb_gadget::function::msd::{Lun, Msd};

#[test]
#[serial]
fn msd() {
    init();

    let mut file1 = NamedTempFile::new().unwrap();
    file1.write_all(&vec![1; 1_048_576]).unwrap();
    let path1 = file1.into_temp_path();

    let mut file2 = NamedTempFile::new().unwrap();
    file2.write_all(&vec![2; 1_048_576]).unwrap();
    let path2 = file2.into_temp_path();

    let mut builder = Msd::builder();
    builder.add_lun(Lun::new(&path1).unwrap());
    let mut lun2 = Lun::new(&path2).unwrap();
    lun2.cdrom = true;
    builder.add_lun(lun2);
    let (msd, func) = builder.build();

    let reg = reg(func);

    println!("MSD device at {}", msd.status().path().unwrap().display());

    check_host(|_device, cfg| {
        // Mass Storage class 8, subclass 6 (SCSI), protocol 0x50 (Bulk-Only Transport).
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 8);
        assert!(intf.is_some(), "no Mass Storage interface (class 8) found on host");
        let intf = intf.unwrap();
        assert_eq!(intf.subclass(), 6, "MSD subclass mismatch (expected SCSI transparent command set)");
        assert_eq!(intf.protocol(), 0x50, "MSD protocol mismatch (expected Bulk-Only Transport)");
        println!(
            "Mass Storage interface {}: class={}, subclass={}, protocol={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
        );

        // Expect 2 bulk endpoints (IN + OUT).
        let num_endpoints = intf.num_endpoints();
        assert_eq!(num_endpoints, 2, "expected 2 bulk endpoints, found {num_endpoints}");
    });

    sleep(Duration::from_secs(1));

    msd.force_eject(0).unwrap();
    sleep(Duration::from_secs(1));

    msd.force_eject(1).unwrap();
    sleep(Duration::from_secs(1));

    msd.set_file(1, Some(&path1)).unwrap();
    sleep(Duration::from_secs(1));

    if unreg(reg).unwrap() {
        path1.close().expect("cannot delete temp file");
        path2.close().expect("cannot delete temp file");
    }
}
