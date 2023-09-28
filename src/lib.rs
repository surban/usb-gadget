//! This library allows implementation of USB peripherals, so called **USB gadgets**,
//! on Linux devices.
//! Both, pre-defined USB functions and fully custom implementations of the USB
//! interface are supported.
//!
//! ### Requirements
//!
//! A USB device controller (UDC) supported by Linux is required.
//!
//! The Linux kernel configuration options `CONFIG_USB_GADGET` and `CONFIG_USB_CONFIGFS`
//! need to be enabled.
//!
//! root permissions are required to configure USB gadgets and
//! the `configfs` filesystem needs to be mounted.
//!
//! ### Usage
//!
//! Start defining an USB gadget by calling [`Gadget::new`].
//! When the gadget is fully specified, call [`Gadget::bind`] to register it with
//! a [USB device controller (UDC)](Udc).
//!

#![warn(missing_docs)]

#[cfg(not(target_os = "linux"))]
compile_error!("usb_gadget only supports Linux");

use proc_mounts::MountIter;
use std::{
    ffi::OsStr,
    io::{Error, ErrorKind, Result},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
};
use tokio::process::Command;

pub mod function;

mod gadget;
pub use gadget::*;

mod udc;
pub use udc::*;

mod lang;
pub use lang::*;

/// USB speed.
#[derive(
    Default, Debug, strum::Display, strum::EnumString, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[non_exhaustive]
pub enum Speed {
    /// USB 3.1: 10 Gbit/s.
    #[strum(serialize = "super-speed-plus")]
    SuperSpeedPlus,
    /// USB 3.0: 5 Gbit/s.
    #[strum(serialize = "super-speed")]
    SuperSpeed,
    /// USB 2.0: 480 Mbit/s.
    #[strum(serialize = "high-speed")]
    HighSpeed,
    /// USB 1.0: 12 Mbit/s.
    #[strum(serialize = "full-speed")]
    FullSpeed,
    /// USB 1.0: 1.5 Mbit/s.
    #[strum(serialize = "low-speed")]
    LowSpeed,
    /// Unknown speed.
    #[default]
    #[strum(serialize = "UNKNOWN")]
    Unknown,
}

/// 8-bit value to hexadecimal notation.
fn hex_u8(value: u8) -> String {
    format!("0x{:02x}", value)
}

/// 16-bit value to hexadecimal notation.
fn hex_u16(value: u16) -> String {
    format!("0x{:04x}", value)
}

/// Returns where configfs is mounted.
fn configfs_dir() -> Result<PathBuf> {
    for mount in MountIter::new()? {
        let Ok(mount) = mount else { continue };
        if mount.fstype == "configfs" {
            return Ok(mount.dest);
        }
    }

    Err(Error::new(ErrorKind::NotFound, "configfs is not mounted"))
}

/// Trims an OsStr.
fn trim_os_str(value: &OsStr) -> &OsStr {
    let mut value = value.as_bytes();

    while value.first() == Some(&b'\n') || value.first() == Some(&b' ') || value.first() == Some(&b'\0') {
        value = &value[1..];
    }

    while value.last() == Some(&b'\n') || value.last() == Some(&b' ') || value.last() == Some(&b'\0') {
        value = &value[..value.len() - 1];
    }

    OsStr::from_bytes(value)
}

/// Request a kernel module to be loaded.
async fn request_module(name: impl AsRef<OsStr>) -> Result<()> {
    let mut res = Command::new("modprobe").arg("-q").arg(name.as_ref()).output().await;

    match res {
        Err(err) if err.kind() == ErrorKind::NotFound => {
            res = Command::new("/sbin/modprobe").arg("-q").arg(name.as_ref()).output().await;
        }
        _ => (),
    }

    match res {
        Ok(out) if out.status.success() => Ok(()),
        Ok(_) => Err(Error::new(ErrorKind::Other, "modprobe failed")),
        Err(err) => Err(err),
    }
}
