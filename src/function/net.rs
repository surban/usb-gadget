//! Net functions.

use macaddr::MacAddr6;
use std::{
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    path::PathBuf,
};

use super::{util::FunctionDir, Function, Handle};
use crate::{gadget::Class, hex_u8};

/// Class of USB network device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetClass {
    /// Ethernet Control Model (CDC ECM).
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_ECM` must be enabled.
    Ecm,
    /// Subset of Ethernet Control Model (CDC ECM subset).
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_ECM_SUBSET` must be enabled.
    EcmSubset,
    /// Ethernet Emulation Model (CDC EEM).
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_EEM` must be enabled.
    Eem,
    /// Network Control Model (CDC NCM).
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_NCM` must be enabled.
    Ncm,
    /// Remote Network Driver Interface Specification (RNDIS).
    ///
    /// The Linux kernel configuration option `CONFIG_USB_CONFIGFS_RNDIS` must be enabled.
    Rndis,
}

impl NetClass {
    fn driver(&self) -> &OsStr {
        OsStr::new(match self {
            NetClass::Ecm => "ecm",
            NetClass::EcmSubset => "geth",
            NetClass::Eem => "eem",
            NetClass::Ncm => "ncm",
            NetClass::Rndis => "rndis",
        })
    }
}

/// Builder for Communication Device Class (CDC) network functions.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct NetBuilder {
    net_class: NetClass,
    /// MAC address of device's end of this Ethernet over USB link.
    pub dev_addr: Option<MacAddr6>,
    /// MAC address of host's end of this Ethernet over USB link.
    pub host_addr: Option<MacAddr6>,
    /// Queue length multiplier for high and super speed.
    pub qmult: Option<u32>,
    /// For RNDIS only: interface class.
    pub interface_class: Option<Class>,
}

impl NetBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Net, Handle) {
        let dir = FunctionDir::new();
        (Net { dir: dir.clone() }, Handle::new(NetFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct NetFunction {
    builder: NetBuilder,
    dir: FunctionDir,
}

impl Function for NetFunction {
    fn driver(&self) -> OsString {
        self.builder.net_class.driver().to_os_string()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if let Some(dev_addr) = self.builder.dev_addr {
            self.dir.write("dev_addr", dev_addr.to_string())?;
        }

        if let Some(host_addr) = self.builder.host_addr {
            self.dir.write("host_addr", host_addr.to_string())?;
        }

        if let Some(qmult) = self.builder.qmult {
            self.dir.write("qmult", qmult.to_string())?;
        }

        if let (NetClass::Rndis, Some(class)) = (self.builder.net_class, self.builder.interface_class) {
            self.dir.write("class", hex_u8(class.class))?;
            self.dir.write("subclass", hex_u8(class.sub_class))?;
            self.dir.write("protocol", hex_u8(class.protocol))?;
        }

        Ok(())
    }
}

/// Communication Device Class (CDC) network function.
#[derive(Debug)]
pub struct Net {
    dir: FunctionDir,
}

impl Net {
    /// Creates a new USB network function.
    pub fn new(net_class: NetClass) -> (Net, Handle) {
        Self::builder(net_class).build()
    }

    /// Creates a new USB network function builder.
    pub fn builder(net_class: NetClass) -> NetBuilder {
        NetBuilder { net_class, dev_addr: None, host_addr: None, qmult: None, interface_class: None }
    }

    /// Path of this USB function in configfs.
    pub fn path(&self) -> Result<PathBuf> {
        self.dir.dir()
    }

    /// MAC address of device's end of this Ethernet over USB link.
    pub fn dev_addr(&self) -> Result<MacAddr6> {
        self.dir.read_string("dev_addr")?.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))
    }

    /// MAC address of host's end of this Ethernet over USB link.
    pub fn host_addr(&self) -> Result<MacAddr6> {
        self.dir.read_string("host_addr")?.parse().map_err(|err| Error::new(ErrorKind::InvalidData, err))
    }

    /// Network device interface name associated with this function instance.
    pub fn ifname(&self) -> Result<OsString> {
        self.dir.read_os_string("ifname")
    }
}
