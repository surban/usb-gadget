use std::{
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
};

use proc_mounts::MountIter;

pub mod function;
pub mod gadget;
pub mod lang;
pub mod udc;

/// 8-bit value to hexadecimal notation.
fn hex_u8(value: u8) -> String {
    format!("0x{:02x}", value)
}

/// 16-bit value to hexadecimal notation.
fn hex_u16(value: u16) -> String {
    format!("0x{:04x}", value)
}

/// Checks that a name is valid for use with USB gadget.
fn check_name(name: &OsStr) -> Result<()> {
    if name.as_bytes().contains(&b'/') || name.as_bytes().contains(&b'.') {
        Err(Error::new(ErrorKind::InvalidInput, "a name must not contain / or ."))
    } else {
        Ok(())
    }
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
