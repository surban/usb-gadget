//! Musical Instrument Digital Interface (MIDI) function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_MIDI` must be enabled.

use std::{
    ffi::OsString,
    io::{Error, ErrorKind, Result},
};

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
    /// Index value for the USB MIDI adapter.
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
        MidiBuilder { buflen: 0, id: String::new(), in_ports: 0, out_ports: 0, index: 0, qlen: 0 }
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }

    /// Device major and minor numbers.
    pub fn device(&self) -> Result<(u32, u32)> {
        let dev = self.dir.read_string("dev")?;
        let Some((major, minor)) = dev.split_once(':') else {
            return Err(Error::new(ErrorKind::InvalidData, "invalid device number format"));
        };
        let major = major.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        let minor = minor.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))?;

        Ok((major, minor))
    }
}
