//! FunctionFS bindings.

use bitflags::bitflags;
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use nix::{
    ioctl_none, ioctl_read, ioctl_write_int_bad,
    mount::{MntFlags, MsFlags},
    request_code_none,
};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fmt,
    io::{ErrorKind, Read, Write},
    num::TryFromIntError,
    os::fd::RawFd,
    path::Path,
};

use crate::{linux_version, Language};

#[derive(Debug, Clone)]
pub enum Error {
    StringsDifferAcrossLanguages,
    Overflow,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::StringsDifferAcrossLanguages => write!(f, "string count differs across languages"),
            Error::Overflow => write!(f, "too many descriptor entries"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        unreachable!()
    }
}

impl From<TryFromIntError> for Error {
    fn from(_: TryFromIntError) -> Self {
        Self::Overflow
    }
}

impl From<Error> for std::io::Error {
    fn from(err: Error) -> Self {
        std::io::Error::new(ErrorKind::InvalidInput, err)
    }
}

type Result<T> = std::result::Result<T, Error>;

/// USB direction to device.
pub const DIR_OUT: u8 = 0x00;
/// USB direction to host.
pub const DIR_IN: u8 = 0x80;

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct Flags: u32 {
        const HAS_FS_DESC = 1;
        const HAS_HS_DESC = 2;
        const HAS_SS_DESC = 4;
        const HAS_MS_OS_DESC = 8;
        const VIRTUAL_ADDR = 16;
        const EVENTFD = 32;
        const ALL_CTRL_RECIP = 64;
        const CONFIG0_SETUP = 128;
    }
}

#[derive(Clone, Debug)]
pub struct Descs {
    pub flags: Flags,
    pub eventfd: Option<RawFd>,
    pub fs_descrs: Vec<Desc>,
    pub hs_descrs: Vec<Desc>,
    pub ss_descrs: Vec<Desc>,
    pub os_descrs: Vec<OsDesc>,
}

impl Descs {
    const MAGIC_V2: u32 = 3;

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        data.write_u32::<LE>(Self::MAGIC_V2)?;
        data.write_u32::<LE>(0)?; // length

        let mut flags = self.flags;
        flags.insert(Flags::HAS_FS_DESC | Flags::HAS_HS_DESC | Flags::HAS_SS_DESC | Flags::HAS_MS_OS_DESC);
        flags.set(Flags::EVENTFD, self.eventfd.is_some());
        data.write_u32::<LE>(flags.bits())?;

        if let Some(fd) = self.eventfd {
            data.write_i32::<LE>(fd)?;
        }

        data.write_u32::<LE>(self.fs_descrs.len().try_into()?)?;
        data.write_u32::<LE>(self.hs_descrs.len().try_into()?)?;
        data.write_u32::<LE>(self.ss_descrs.len().try_into()?)?;
        data.write_u32::<LE>(self.os_descrs.len().try_into()?)?;

        for fs_descr in &self.fs_descrs {
            data.extend(fs_descr.to_bytes()?);
        }
        for hs_descr in &self.hs_descrs {
            data.extend(hs_descr.to_bytes()?);
        }
        for ss_descr in &self.ss_descrs {
            data.extend(ss_descr.to_bytes()?);
        }
        for os_descr in &self.os_descrs {
            data.extend(os_descr.to_bytes()?);
        }

        let len: u32 = data.len().try_into()?;
        data[4..8].copy_from_slice(&len.to_le_bytes());

        Ok(data)
    }
}

#[derive(Clone, Debug)]
pub enum Desc {
    Interface(InterfaceDesc),
    Endpoint(EndpointDesc),
    SsEndpointComp(SsEndpointComp),
    InterfaceAssoc(InterfaceAssocDesc),
    Custom(CustomDesc),
}

impl From<InterfaceDesc> for Desc {
    fn from(value: InterfaceDesc) -> Self {
        Self::Interface(value)
    }
}

impl From<EndpointDesc> for Desc {
    fn from(value: EndpointDesc) -> Self {
        Self::Endpoint(value)
    }
}

impl From<SsEndpointComp> for Desc {
    fn from(value: SsEndpointComp) -> Self {
        Self::SsEndpointComp(value)
    }
}

impl From<InterfaceAssocDesc> for Desc {
    fn from(value: InterfaceAssocDesc) -> Self {
        Self::InterfaceAssoc(value)
    }
}

impl From<CustomDesc> for Desc {
    fn from(value: CustomDesc) -> Self {
        Self::Custom(value)
    }
}

impl Desc {
    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        data.write_u8(0)?;

        match self {
            Self::Interface(d) => d.write(&mut data)?,
            Self::Endpoint(d) => d.write(&mut data)?,
            Self::SsEndpointComp(d) => d.write(&mut data)?,
            Self::InterfaceAssoc(d) => d.write(&mut data)?,
            Self::Custom(d) => d.write(&mut data)?,
        }

        data[0] = data.len().try_into()?;
        Ok(data)
    }
}

#[derive(Clone, Debug)]
pub struct InterfaceDesc {
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_sub_class: u8,
    pub interface_protocol: u8,
    pub name_idx: u8,
}

impl InterfaceDesc {
    pub const TYPE: u8 = 0x04;

    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        data.write_u8(Self::TYPE)?;
        data.write_u8(self.interface_number)?;
        data.write_u8(self.alternate_setting)?;
        data.write_u8(self.num_endpoints)?;
        data.write_u8(self.interface_class)?;
        data.write_u8(self.interface_sub_class)?;
        data.write_u8(self.interface_protocol)?;
        data.write_u8(self.name_idx)?;
        Ok(())
    }
}

/// USB endpoint descriptor.
#[derive(Clone, Debug)]
pub struct EndpointDesc {
    /// Endpoint address.
    pub endpoint_address: u8,
    /// Attributes.
    pub attributes: u8,
    /// Maximum packet size.
    pub max_packet_size: u16,
    /// Interval.
    pub interval: u8,
    /// Audio-endpoint specific data.
    pub audio: Option<AudioEndpointDesc>,
}

/// Audio-part of USB endpoint descriptor.
#[derive(Clone, Debug)]
pub struct AudioEndpointDesc {
    /// Refresh.
    pub refresh: u8,
    /// Sync address.
    pub synch_address: u8,
}

impl EndpointDesc {
    /// Endpoint descriptor type.
    pub const TYPE: u8 = 0x05;

    /// Size without audio.
    pub const SIZE: usize = 7;
    /// Size with audio.
    pub const AUDIO_SIZE: usize = 9;

    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        data.write_u8(Self::TYPE)?;
        data.write_u8(self.endpoint_address)?;
        data.write_u8(self.attributes)?;
        data.write_u16::<LE>(self.max_packet_size)?;
        data.write_u8(self.interval)?;

        if let Some(audio) = &self.audio {
            data.write_u8(audio.refresh)?;
            data.write_u8(audio.synch_address)?;
        }

        Ok(())
    }

    /// Parse from raw descriptor data.
    pub fn parse(mut data: &[u8]) -> std::io::Result<Self> {
        let size = data.read_u8()?;
        if usize::from(size) != Self::SIZE && usize::from(size) != Self::AUDIO_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "endpoint descriptor size mismatch",
            ));
        }

        if data.read_u8()? != Self::TYPE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "endpoint descriptor type mismatch",
            ));
        }

        let endpoint_address = data.read_u8()?;
        let attributes = data.read_u8()?;
        let max_packet_size = data.read_u16::<LE>()?;
        let interval = data.read_u8()?;

        let audio = if usize::from(size) == Self::AUDIO_SIZE {
            let refresh = data.read_u8()?;
            let synch_address = data.read_u8()?;
            Some(AudioEndpointDesc { refresh, synch_address })
        } else {
            None
        };

        Ok(Self { endpoint_address, attributes, max_packet_size, interval, audio })
    }
}

#[derive(Clone, Debug)]
pub struct SsEndpointComp {
    pub max_burst: u8,
    pub attributes: u8,
    pub bytes_per_interval: u16,
}

impl SsEndpointComp {
    pub const TYPE: u8 = 0x30;

    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        data.write_u8(Self::TYPE)?;
        data.write_u8(self.max_burst)?;
        data.write_u8(self.attributes)?;
        data.write_u16::<LE>(self.bytes_per_interval)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct InterfaceAssocDesc {
    pub first_interface: u8,
    pub interface_count: u8,
    pub function_class: u8,
    pub function_sub_class: u8,
    pub function_protocol: u8,
    pub name_idx: u8,
}

impl InterfaceAssocDesc {
    pub const TYPE: u8 = 0x0b;

    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        data.write_u8(Self::TYPE)?;
        data.write_u8(self.first_interface)?;
        data.write_u8(self.interface_count)?;
        data.write_u8(self.function_class)?;
        data.write_u8(self.function_sub_class)?;
        data.write_u8(self.function_protocol)?;
        data.write_u8(self.name_idx)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct OsDesc {
    pub interface: u8,
    pub ext: OsDescExt,
}

impl OsDesc {
    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.write_u8(self.interface)?;
        data.write_u32::<LE>(0)?; // length

        self.ext.write(&mut data)?;

        let len: u32 = data.len().try_into()?;
        data[1..5].copy_from_slice(&len.to_le_bytes());
        Ok(data)
    }
}

#[derive(Clone, Debug)]
pub enum OsDescExt {
    ExtCompat(Vec<OsExtCompat>),
    ExtProp(Vec<OsExtProp>),
}

impl OsDescExt {
    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        let os_desc_bcd_version = if linux_version().unwrap_or_default() >= (6, 4) { 0x0100 } else { 0x0001 };

        match self {
            Self::ExtCompat(compats) => {
                data.write_u16::<LE>(os_desc_bcd_version)?; // bcdVersion
                data.write_u16::<LE>(4)?; // wIndex
                data.write_u8(compats.len().try_into()?)?;
                data.write_u8(0)?;

                for compat in compats {
                    compat.write(data)?;
                }
            }
            Self::ExtProp(props) => {
                data.write_u16::<LE>(os_desc_bcd_version)?; // bcdVersion
                data.write_u16::<LE>(5)?; // wIndex
                data.write_u16::<LE>(props.len().try_into()?)?;

                for prop in props {
                    data.extend(prop.to_bytes()?);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct OsExtCompat {
    pub first_interface_number: u8,
    pub compatible_id: [u8; 8],
    pub sub_compatible_id: [u8; 8],
}

impl OsExtCompat {
    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        data.write_u8(self.first_interface_number)?;
        data.write_u8(1)?;
        data.extend_from_slice(&self.compatible_id);
        data.extend_from_slice(&self.sub_compatible_id);
        data.extend_from_slice(&[0; 6]);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct OsExtProp {
    pub data_type: u32,
    pub name: String,
    pub data: Vec<u8>,
}

impl OsExtProp {
    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        data.write_u32::<LE>(0)?; // length
        data.write_u32::<LE>(self.data_type)?;
        data.write_u16::<LE>(self.name.len().try_into()?)?;
        data.write_all(self.name.as_bytes())?;
        data.write_u32::<LE>(self.data.len().try_into()?)?;
        data.write_all(&self.data)?;

        let len: u32 = data.len().try_into()?;
        assert_eq!(len as usize, 14 + self.name.len() + self.data.len(), "invalid OS descriptor length");
        data[0..4].copy_from_slice(&len.to_le_bytes());
        Ok(data)
    }
}

/// Custom descriptor.
///
/// The raw descriptor published will be of the form:
/// `[length, descriptor_type, data...]`
#[derive(Clone, Debug)]
pub struct CustomDesc {
    /// Descriptor type.
    pub descriptor_type: u8,
    /// Custom data.
    pub data: Vec<u8>,
}

impl CustomDesc {
    /// Creates a new custom descriptor.
    ///
    /// The data must not include the length and descriptor type.
    pub fn new(descriptor_type: u8, data: Vec<u8>) -> Self {
        Self { descriptor_type, data }
    }

    fn write(&self, data: &mut Vec<u8>) -> Result<()> {
        data.write_u8(self.descriptor_type)?;
        data.write_all(&self.data)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Strings(pub HashMap<Language, Vec<String>>);

impl Strings {
    const MAGIC: u32 = 2;

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let str_count = self.0.values().next().map(|v| v.len()).unwrap_or_default();
        if !self.0.values().all(|v| v.len() == str_count) {
            return Err(Error::StringsDifferAcrossLanguages);
        }

        let mut data = Vec::new();

        data.write_u32::<LE>(Self::MAGIC)?;
        data.write_u32::<LE>(0)?; // length
        data.write_u32::<LE>(str_count.try_into()?)?;
        data.write_u32::<LE>(self.0.len().try_into()?)?;

        for (lang, strings) in &self.0 {
            data.write_u16::<LE>((*lang).into())?;
            for str in strings {
                data.write_all(str.as_bytes())?;
                data.write_u8(0)?;
            }
        }

        let len: u32 = data.len().try_into()?;
        data[4..8].copy_from_slice(&len.to_le_bytes());
        Ok(data)
    }
}

/// USB control request.
#[derive(Clone, Debug)]
pub struct CtrlReq {
    /// Request type.
    pub request_type: u8,
    /// Request.
    pub request: u8,
    /// Value.
    pub value: u16,
    /// Index.
    pub index: u16,
    /// Length.
    pub length: u16,
}

impl CtrlReq {
    /// Parse from buffer.
    pub fn parse(mut buf: &[u8]) -> Result<Self> {
        let request_type = buf.read_u8()?;
        let request = buf.read_u8()?;
        let value = buf.read_u16::<LE>()?;
        let index = buf.read_u16::<LE>()?;
        let length = buf.read_u16::<LE>()?;
        Ok(Self { request_type, request, value, index, length })
    }
}

/// FunctionFS event type.
pub mod event {
    pub const BIND: u8 = 0;
    pub const UNBIND: u8 = 1;
    pub const ENABLE: u8 = 2;
    pub const DISABLE: u8 = 3;
    pub const SETUP: u8 = 4;
    pub const SUSPEND: u8 = 5;
    pub const RESUME: u8 = 6;
}

/// FunctionFS event.
pub struct Event {
    pub data: [u8; 8],
    pub event_type: u8,
}

impl Event {
    /// Size of raw event data.
    pub const SIZE: usize = 12;

    /// Parse FunctionFS event data.
    pub fn parse(mut buf: &[u8]) -> Result<Self> {
        let mut data = [0; 8];
        buf.read_exact(&mut data)?;
        let event_type = buf.read_u8()?;
        let mut pad = [0; 3];
        buf.read_exact(&mut pad)?;
        Ok(Self { data, event_type })
    }
}

pub const FS_TYPE: &str = "functionfs";

/// FunctionFS mount options.
#[derive(Debug, Clone, Default)]
pub struct MountOptions {
    pub no_disconnect: bool,
    pub rmode: Option<u32>,
    pub fmode: Option<u32>,
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl MountOptions {
    fn to_mount_data(&self) -> String {
        let mut opts = Vec::new();

        if self.no_disconnect {
            opts.push("no_disconnect=1".to_string());
        }
        if let Some(v) = self.rmode {
            opts.push(format!("rmode={v}"));
        }
        if let Some(v) = self.fmode {
            opts.push(format!("fmode={v}"));
        }
        if let Some(v) = self.mode {
            opts.push(format!("mode={v}"));
        }
        if let Some(v) = self.uid {
            opts.push(format!("uid={v}"));
        }
        if let Some(v) = self.gid {
            opts.push(format!("gid={v}"));
        }

        opts.join(",")
    }
}

pub fn mount(instance: &OsStr, target: &Path, opts: &MountOptions) -> std::io::Result<()> {
    let instance = instance.to_os_string();
    let target = target.to_path_buf();
    nix::mount::mount(
        Some(instance.as_os_str()),
        &target,
        Some(FS_TYPE),
        MsFlags::empty(),
        Some(opts.to_mount_data().as_str()),
    )?;
    Ok(())
}

pub fn umount(target: &Path, lazy: bool) -> std::io::Result<()> {
    let target = target.to_path_buf();
    nix::mount::umount2(&target, if lazy { MntFlags::MNT_DETACH } else { MntFlags::empty() })?;
    Ok(())
}

ioctl_none!(fifo_status, 'g', 1);
ioctl_none!(fifo_flush, 'g', 2);
ioctl_none!(clear_halt, 'g', 3);

ioctl_write_int_bad!(interface_revmap, request_code_none!('g', 128));
ioctl_none!(endpoint_revmap, 'g', 129);
ioctl_read!(endpoint_desc, 'g', 130, [u8; EndpointDesc::AUDIO_SIZE]);
