mod common;
use common::*;

use std::{io::Write, thread::sleep, time::Duration};
use tempfile::NamedTempFile;

use usb_gadget::function::msd::{Lun, Msd};

#[test]
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

    println!("MSD device at {}", msd.path().unwrap().display());

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
