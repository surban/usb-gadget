//! Printer function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_PRINTER` must be enabled.
//!
//! A sysfs file at /dev/g_printerN will be created for each instance of the function, where N instance number. See https://docs.kernel.org/usb/gadget_printer.html#using-the-example-code for how to use this file.

use std::{
    ffi::OsString,
    io::Result,
};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Builder for USB human interface device (PRINTER) function.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PrinterBuilder {
    /// The PNP ID string used for this printer.
    pub pnp_string: Option<String>,
    /// The number of 8k buffers to use per endpoint. The default is 10.
    pub qlen: Option<u8>
}

impl PrinterBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Printer, Handle) {
        let dir = FunctionDir::new();
        (Printer { dir: dir.clone() }, Handle::new(PrinterFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct PrinterFunction {
    builder: PrinterBuilder,
    dir: FunctionDir,
}

impl Function for PrinterFunction {
    fn driver(&self) -> OsString {
        "printer".into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if let Some(pnp_string) = &self.builder.pnp_string {
            self.dir.write("pnp_string", pnp_string)?;
        }
        if let Some(qlen) = self.builder.qlen {
            self.dir.write("q_len", qlen.to_string())?;
        }

        Ok(())
    }
}

/// USB human interface device (PRINTER) function.
#[derive(Debug)]
pub struct Printer {
    dir: FunctionDir,
}

impl Printer {
    /// Creates a new USB human interface device (PRINTER) builder.
    pub fn builder() -> PrinterBuilder {
        PrinterBuilder {
            pnp_string: None,
            qlen: None,
        }
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }
}
