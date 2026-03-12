//! Loopback function (for testing).
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_LB_SS` must be enabled.
//! The loopback function loops back a configurable number of transfers, useful for testing
//! USB device controllers and host-side software.
//!
//! # Example
//!
//! ```no_run
//! use usb_gadget::{
//!     default_udc, function::loopback::Loopback, Class, Config, Gadget, Id, Strings,
//! };
//!
//! let (loopback, func) = Loopback::new();
//!
//! let udc = default_udc().expect("cannot get UDC");
//! let reg = Gadget::new(
//!     Class::vendor_specific(0, 0),
//!     Id::LINUX_FOUNDATION_COMPOSITE,
//!     Strings::new("Manufacturer", "Loopback", "0123456"),
//! )
//! .with_config(Config::new("Loopback Config 1").with_function(func))
//! .bind(&udc)
//! .expect("cannot bind to UDC");
//!
//! println!(
//!     "Loopback {} at {} status {:?}",
//!     reg.name().to_string_lossy(),
//!     reg.path().display(),
//!     loopback.status()
//! );
//! ```

use std::{ffi::OsString, io::Result};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Builder for USB loopback function.
///
/// None values will use the f_loopback module defaults.
/// See `drivers/usb/gadget/function/f_loopback.c`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct LoopbackBuilder {
    /// Number of requests to allocate per endpoint.
    pub qlen: Option<u32>,
    /// Size of each bulk transfer buffer in bytes.
    pub bulk_buflen: Option<u32>,
}

impl LoopbackBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Loopback, Handle) {
        let dir = FunctionDir::new();
        (Loopback { dir: dir.clone() }, Handle::new(LoopbackFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct LoopbackFunction {
    builder: LoopbackBuilder,
    dir: FunctionDir,
}

impl Function for LoopbackFunction {
    fn driver(&self) -> OsString {
        "Loopback".into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if let Some(qlen) = self.builder.qlen {
            self.dir.write("qlen", qlen.to_string())?;
        }
        if let Some(bulk_buflen) = self.builder.bulk_buflen {
            self.dir.write("bulk_buflen", bulk_buflen.to_string())?;
        }

        Ok(())
    }
}

/// USB loopback function.
///
/// Loops back a configurable number of bulk transfers. Useful for testing
/// USB device controllers with host-side test software like the `usbtest` driver.
#[derive(Debug)]
pub struct Loopback {
    dir: FunctionDir,
}

impl Loopback {
    /// Creates a new USB loopback function with default settings.
    pub fn new() -> (Loopback, Handle) {
        Self::builder().build()
    }

    /// Creates a new USB loopback function builder.
    pub fn builder() -> LoopbackBuilder {
        LoopbackBuilder::default()
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }
}
