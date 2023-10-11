//! Serial functions.

use std::{
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    path::PathBuf,
};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Class of USB serial function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SerialClass {
    /// Abstract Control Model (CDC ACM).
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_ACM` must be enabled.
    Acm,
    /// Generic serial.
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_SERIAL` must be enabled.
    Generic,
}

impl SerialClass {
    fn driver(&self) -> &OsStr {
        OsStr::new(match self {
            SerialClass::Acm => "acm",
            SerialClass::Generic => "gser",
        })
    }
}

/// Builder for USB serial function.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SerialBuilder {
    serial_class: SerialClass,
    /// Console?
    pub console: Option<bool>,
}

impl SerialBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Serial, Handle) {
        let dir = FunctionDir::new();
        (Serial { dir: dir.clone() }, Handle::new(SerialFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct SerialFunction {
    builder: SerialBuilder,
    dir: FunctionDir,
}

impl Function for SerialFunction {
    fn driver(&self) -> OsString {
        self.builder.serial_class.driver().to_os_string()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if let Some(console) = self.builder.console {
            // Console support is optional.
            let _ = self.dir.write("console", if console { "1" } else { "0" });
        }

        Ok(())
    }
}

/// USB serial function.
#[derive(Debug)]
pub struct Serial {
    dir: FunctionDir,
}

impl Serial {
    /// Creates a new USB serial function.
    pub fn new(serial_class: SerialClass) -> (Serial, Handle) {
        Self::builder(serial_class).build()
    }

    /// Creates a new USB serial function builder.
    pub fn builder(serial_class: SerialClass) -> SerialBuilder {
        SerialBuilder { serial_class, console: None }
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }

    /// Path to TTY device.
    pub fn tty(&self) -> Result<PathBuf> {
        let port_num: u32 =
            self.dir.read_string("port_num")?.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        Ok(format!("/dev/ttyGS{port_num}").into())
    }
}
