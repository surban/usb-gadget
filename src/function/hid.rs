//! Human interface device (HID) function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_HID` must be enabled.

use std::{
    ffi::OsString,
    io::{Error, ErrorKind, Result},
};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Builder for USB human interface device (HID) function.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HidBuilder {
    /// HID subclass to use.
    pub sub_class: u8,
    /// HID protocol to use.
    pub protocol: u8,
    /// Data to be used in HID reports.
    pub report_desc: Vec<u8>,
    /// HID report length.
    pub report_len: u8,
    /// No out endpoint?
    pub no_out_endpoint: bool,
}

impl HidBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Hid, Handle) {
        let dir = FunctionDir::new();
        (Hid { dir: dir.clone() }, Handle::new(HidFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct HidFunction {
    builder: HidBuilder,
    dir: FunctionDir,
}

impl Function for HidFunction {
    fn driver(&self) -> OsString {
        "hid".into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        self.dir.write("subclass", self.builder.sub_class.to_string())?;
        self.dir.write("protocol", self.builder.protocol.to_string())?;
        self.dir.write("report_desc", &self.builder.report_desc)?;
        self.dir.write("report_length", self.builder.report_len.to_string())?;
        self.dir.write("no_out_endpoint", if self.builder.no_out_endpoint { "1" } else { "0" })?;

        Ok(())
    }
}

/// USB human interface device (HID) function.
#[derive(Debug)]
pub struct Hid {
    dir: FunctionDir,
}

impl Hid {
    /// Creates a new USB human interface device (HID) builder.
    pub fn builder() -> HidBuilder {
        HidBuilder { sub_class: 0, protocol: 0, report_desc: Vec::new(), report_len: 0, no_out_endpoint: false }
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }

    /// Device major and minor numbers.
    pub fn device(&self) -> Result<(u8, u8)> {
        let dev = self.dir.read_string("dev")?;
        let Some((major, minor)) = dev.split_once(':') else {
            return Err(Error::new(ErrorKind::InvalidData, "invalid device number format"));
        };
        let major = major.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        let minor = minor.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))?;

        Ok((major, minor))
    }
}
