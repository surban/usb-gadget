//! Musical Instrument Digital Interface (MIDI) function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_MIDI` must be enabled. Can use `amidi -l` once the gadget is configured to list the MIDI devices.
//!
//! # Example
//!
//! ```no_run
//! use usb_gadget::function::midi::Midi;
//! use usb_gadget::{default_udc, Class, Config, Gadget, Id, Strings};
//!
//! let mut builder = Midi::builder();
//! // This must be an available sound device index - see docs on index for more information
//! builder.index = 0;
//! builder.id = "midi".to_string();
//! builder.in_ports = 1;
//! builder.out_ports = 1;
//! let (midi, func) = builder.build();
//!
//! let udc = default_udc().expect("cannot get UDC");
//! let reg =
//!     // USB device descriptor base class 0, 0, 0: use Interface Descriptors
//!     // Linux Foundation VID Gadget PID
//!     Gadget::new(Class::new(0, 0, 0), Id::new(0x1d6b, 0x0104), Strings::new("Clippy Manufacturer", "Rust MIDI", "RUST0123456"))
//!         .with_config(Config::new("MIDI Config 1").with_function(func))
//!         .bind(&udc)
//!         .expect("cannot bind to UDC");
//!
//! println!(
//!     "USB MIDI {} at {} to {} status {:?}",
//!     reg.name().to_string_lossy(),
//!     reg.path().display(),
//!     udc.name().to_string_lossy(),
//!     midi.status()
//! );
//! ```


use std::{ffi::OsString, io::Result};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Builder for USB musical instrument digital interface (MIDI) function.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MidiBuilder {
    /// MIDI buffer length
    pub buflen: u16,
    /// ID string for the USB MIDI adapter
    pub id: String,
    /// Number of MIDI input ports
    pub in_ports: u8,
    /// Number of MIDI output ports
    pub out_ports: u8,
    /// Sound device index for the MIDI adapter
    ///
    /// There must be a sound device available with this index. If the device fails to register and in dmesg one sees 'cannot find the slot for index $index (range 0-1), error: -16', then the index is not available. Most likely the index is already in use by the physical sound card. Try another index within range or unload the physical sound card driver. See [USB Gadget MIDI](https://linux-sunxi.org/USB_Gadget/MIDI).
    pub index: u8,
    /// USB read request queue length
    pub qlen: u8,
}

impl MidiBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Midi, Handle) {
        let dir = FunctionDir::new();
        (Midi { dir: dir.clone() }, Handle::new(MidiFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct MidiFunction {
    builder: MidiBuilder,
    dir: FunctionDir,
}

impl Function for MidiFunction {
    fn driver(&self) -> OsString {
        "midi".into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        self.dir.write("buflen", self.builder.buflen.to_string())?;
        self.dir.write("id", &self.builder.id)?;
        self.dir.write("in_ports", &self.builder.in_ports.to_string())?;
        self.dir.write("out_ports", self.builder.out_ports.to_string())?;
        self.dir.write("index", self.builder.index.to_string())?;
        self.dir.write("qlen", self.builder.qlen.to_string())?;

        Ok(())
    }
}

/// USB musical instrument digital interface (MIDI) function.
#[derive(Debug)]
pub struct Midi {
    dir: FunctionDir,
}

impl Midi {
    /// Creates a new USB musical instrument digital interface (MIDI) builder.
    pub fn builder() -> MidiBuilder {
        MidiBuilder { buflen: 512, id: String::new(), in_ports: 1, out_ports: 1, index: 0, qlen: 32 }
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }
}
