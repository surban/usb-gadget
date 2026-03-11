//! Source/Sink function (for testing).
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_LB_SS` must be enabled.
//! The source/sink function either sinks and sources bulk data, and also supports
//! isochronous transfers. It implements control requests for "chapter 9" conformance
//! testing.
//!
//! # Example
//!
//! ```no_run
//! use usb_gadget::{
//!     default_udc, function::sourcesink::SourceSink, Class, Config, Gadget, Id, Strings,
//! };
//!
//! let (ss, func) = SourceSink::new();
//!
//! let udc = default_udc().expect("cannot get UDC");
//! let reg = Gadget::new(
//!     Class::new(0xff, 0, 0),
//!     Id::new(0x1d6b, 0x0104),
//!     Strings::new("Manufacturer", "SourceSink", "0123456"),
//! )
//! .with_config(Config::new("SourceSink Config 1").with_function(func))
//! .bind(&udc)
//! .expect("cannot bind to UDC");
//!
//! println!(
//!     "SourceSink {} at {} status {:?}",
//!     reg.name().to_string_lossy(),
//!     reg.path().display(),
//!     ss.status()
//! );
//! ```

use std::{ffi::OsString, io::Result};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Builder for USB source/sink function.
///
/// None values will use the f_sourcesink module defaults.
/// See `drivers/usb/gadget/function/f_sourcesink.c`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SourceSinkBuilder {
    /// Data pattern to use (0 = all zeros, 1 = mod63, 2 = none).
    pub pattern: Option<u32>,
    /// Isochronous transfer polling interval (1..16).
    pub isoc_interval: Option<u32>,
    /// Maximum packet size for isochronous endpoints.
    pub isoc_maxpacket: Option<u32>,
    /// Isochronous transfer multiplier (0..2, for HS/SS).
    pub isoc_mult: Option<u32>,
    /// Maximum burst for isochronous endpoints (0..15, for SS).
    pub isoc_maxburst: Option<u32>,
    /// Size of each bulk transfer buffer in bytes.
    pub bulk_buflen: Option<u32>,
    /// Number of bulk request buffers to allocate.
    pub bulk_qlen: Option<u32>,
    /// Number of isochronous request buffers to allocate.
    pub iso_qlen: Option<u32>,
}

impl SourceSinkBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (SourceSink, Handle) {
        let dir = FunctionDir::new();
        (SourceSink { dir: dir.clone() }, Handle::new(SourceSinkFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct SourceSinkFunction {
    builder: SourceSinkBuilder,
    dir: FunctionDir,
}

impl Function for SourceSinkFunction {
    fn driver(&self) -> OsString {
        "SourceSink".into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if let Some(pattern) = self.builder.pattern {
            self.dir.write("pattern", pattern.to_string())?;
        }
        if let Some(isoc_interval) = self.builder.isoc_interval {
            self.dir.write("isoc_interval", isoc_interval.to_string())?;
        }
        if let Some(isoc_maxpacket) = self.builder.isoc_maxpacket {
            self.dir.write("isoc_maxpacket", isoc_maxpacket.to_string())?;
        }
        if let Some(isoc_mult) = self.builder.isoc_mult {
            self.dir.write("isoc_mult", isoc_mult.to_string())?;
        }
        if let Some(isoc_maxburst) = self.builder.isoc_maxburst {
            self.dir.write("isoc_maxburst", isoc_maxburst.to_string())?;
        }
        if let Some(bulk_buflen) = self.builder.bulk_buflen {
            self.dir.write("bulk_buflen", bulk_buflen.to_string())?;
        }
        if let Some(bulk_qlen) = self.builder.bulk_qlen {
            self.dir.write("bulk_qlen", bulk_qlen.to_string())?;
        }
        if let Some(iso_qlen) = self.builder.iso_qlen {
            self.dir.write("iso_qlen", iso_qlen.to_string())?;
        }

        Ok(())
    }
}

/// USB source/sink function.
///
/// Sources and sinks bulk data with configurable isochronous support.
/// Useful for USB conformance testing and benchmarking with host-side tools
/// like the `usbtest` driver.
#[derive(Debug)]
pub struct SourceSink {
    dir: FunctionDir,
}

impl SourceSink {
    /// Creates a new USB source/sink function with default settings.
    pub fn new() -> (SourceSink, Handle) {
        Self::builder().build()
    }

    /// Creates a new USB source/sink function builder.
    pub fn builder() -> SourceSinkBuilder {
        SourceSinkBuilder::default()
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }
}
