//! Printer example userspace application based on [prn_example](https://docs.kernel.org/6.6/usb/gadget_printer.html#example-code)
//!
//! Creates and binds a printer gadget function, then reads data from the device file created by the gadget to stdout. Will exit after printing a set number of pages.
use nix::{ioctl_read, ioctl_readwrite};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use usb_gadget::function::printer::{Printer, StatusFlags, GADGET_GET_PRINTER_STATUS, GADGET_SET_PRINTER_STATUS};
use usb_gadget::{default_udc, Class, Config, Gadget, Id, RegGadget, Strings, GADGET_IOC_MAGIC};

// Printer read buffer size, best equal to EP wMaxPacketSize
const BUF_SIZE: usize = 512;
// Printer sysfs path - 0 assumes we are the only printer gadget!
const SYSFS_PATH: &str = "/dev/g_printer0";
// Pages to 'print' before exiting
const PRINT_EXIT_COUNT: u8 = 1;
// Default printer status
const DEFAULT_STATUS: StatusFlags =
    StatusFlags::from_bits_truncate(StatusFlags::NOT_ERROR.bits() | StatusFlags::SELECTED.bits());

// ioctl read/write for printer status
ioctl_read!(ioctl_read_printer_status, GADGET_IOC_MAGIC, GADGET_GET_PRINTER_STATUS, u8);
ioctl_readwrite!(ioctl_write_printer_status, GADGET_IOC_MAGIC, GADGET_SET_PRINTER_STATUS, u8);

fn create_printer_gadget() -> io::Result<RegGadget> {
    usb_gadget::remove_all().expect("cannot remove all gadgets");

    let udc = default_udc().expect("cannot get UDC");
    let mut builder = Printer::builder();
    builder.pnp_string = Some("Rust PNP".to_string());

    let (_, func) = builder.build();
    let reg =
        // Linux Foundation VID Gadget PID
        Gadget::new(Class::interface_specific(), Id::new(0x1d6b, 0x0104), Strings::new("Clippy Manufacturer", "Rusty Printer", "RUST0123456"))
            .with_config(Config::new("Config 1")
                .with_function(func))
            .bind(&udc)?;

    Ok(reg)
}

fn read_printer_data(file: &mut File) -> io::Result<()> {
    let mut buf = [0u8; BUF_SIZE];
    let mut printed = 0;
    println!("Will exit after printing {} pages...", PRINT_EXIT_COUNT);

    loop {
        let bytes_read = match file.read(&mut buf) {
            Ok(bytes_read) if bytes_read > 0 => bytes_read,
            _ => break,
        };
        io::stdout().write_all(&buf[..bytes_read])?;
        io::stdout().flush()?;

        // check if %%EOF is in the buffer
        if buf.windows(5).any(|w| w == b"%%EOF") {
            printed += 1;
            if printed == PRINT_EXIT_COUNT {
                println!("Printed {} pages, exiting.", PRINT_EXIT_COUNT);
                break;
            }
        }
    }

    Ok(())
}

fn set_printer_status(file: &File, flags: StatusFlags, clear: bool) -> io::Result<StatusFlags> {
    let mut status = get_printer_status(file)?;
    if clear {
        status.remove(flags);
    } else {
        status.insert(flags);
    }
    let mut bits = status.bits();
    log::debug!("Setting printer status: {:08b}", bits);
    unsafe { ioctl_write_printer_status(file.as_raw_fd(), &mut bits) }?;
    Ok(StatusFlags::from_bits_truncate(bits))
}

fn get_printer_status(file: &File) -> io::Result<StatusFlags> {
    let mut status = 0;
    unsafe { ioctl_read_printer_status(file.as_raw_fd(), &mut status) }?;
    log::debug!("Got printer status: {:08b}", status);
    let status = StatusFlags::from_bits_truncate(status);
    Ok(status)
}

fn print_status(status: StatusFlags) {
    println!("Printer status is:");
    if status.contains(StatusFlags::SELECTED) {
        println!("     Printer is Selected");
    } else {
        println!("     Printer is NOT Selected");
    }
    if status.contains(StatusFlags::PAPER_EMPTY) {
        println!("     Paper is Out");
    } else {
        println!("     Paper is Loaded");
    }
    if status.contains(StatusFlags::NOT_ERROR) {
        println!("     Printer OK");
    } else {
        println!("     Printer ERROR");
    }
}

fn main() -> io::Result<()> {
    env_logger::init();

    // create var printer gadget, will unbind on drop
    let g_printer = create_printer_gadget().map_err(|e| {
        eprintln!("Failed to create printer gadget: {:?}", e);
        e
    })?;
    println!("Printer gadget created: {:?}", g_printer.path());

    // wait for sysfs device to create
    let mut count = 0;
    let mut sysfs_path = None;
    println!("Attempt open sysfs path: {}", SYSFS_PATH);
    while count < 5 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        // test open access
        if let Ok(_) = OpenOptions::new().read(true).write(true).open(&SYSFS_PATH) {
            sysfs_path = Some(PathBuf::from(SYSFS_PATH));
            break;
        }
        count += 1;
    }

    match sysfs_path {
        Some(pp) => {
            let mut file = OpenOptions::new().read(true).write(true).open(&pp)?;

            print_status(set_printer_status(&file, DEFAULT_STATUS, false)?);
            if let Err(e) = read_printer_data(&mut file) {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to read data from {}: {:?}", pp.display(), e),
                ))
            } else {
                Ok(())
            }
        }
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Printer {} not found or cannot open", SYSFS_PATH),
        )),
    }
}
