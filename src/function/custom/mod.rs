//! Custom USB interface, implemented in user code.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_FS` must be enabled.

use bytes::{Bytes, BytesMut};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use proc_mounts::MountIter;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    ffi::{OsStr, OsString},
    fmt, fs,
    fs::File,
    hash::Hash,
    io::{Error, ErrorKind, Read, Result, Write},
    os::fd::{AsFd, AsRawFd, RawFd},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};
use uuid::Uuid;

use super::{
    util::{split_function_dir, value, FunctionDir, Status},
    Function, Handle,
};
use crate::{Class, Language};

mod aio;
mod ffs;

pub(crate) fn driver() -> &'static OsStr {
    OsStr::new("ffs")
}

pub use ffs::CustomDesc;

/// An USB interface.
#[derive(Debug)]
#[non_exhaustive]
pub struct Interface {
    /// Interface class.
    pub interface_class: Class,
    /// Interface name.
    pub name: HashMap<Language, String>,
    /// USB endpoints.
    pub endpoints: Vec<Endpoint>,
    /// Interface association.
    pub association: Option<Association>,
    /// Microsoft extended compatibility descriptors.
    pub os_ext_compat: Vec<OsExtCompat>,
    /// Microsoft extended properties.
    pub os_ext_props: Vec<OsExtProp>,
    /// Custom descriptors.
    ///
    /// These are inserted directly after the interface descriptor.
    pub custom_descs: Vec<CustomDesc>,
}

impl Interface {
    /// Creates a new interface.
    pub fn new(interface_class: Class, name: impl AsRef<str>) -> Self {
        Self {
            interface_class,
            name: [(Language::default(), name.as_ref().to_string())].into(),
            endpoints: Vec::new(),
            association: None,
            os_ext_compat: Vec::new(),
            os_ext_props: Vec::new(),
            custom_descs: Vec::new(),
        }
    }

    /// Add an USB endpoint.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// Set the USB interface association.
    #[must_use]
    pub fn with_association(mut self, association: &Association) -> Self {
        self.association = Some(association.clone());
        self
    }

    /// Adds a Microsoft extended compatibility descriptor.
    #[must_use]
    pub fn with_os_ext_compat(mut self, os_ext_compat: OsExtCompat) -> Self {
        self.os_ext_compat.push(os_ext_compat);
        self
    }

    /// Adds a Microsoft extended property.
    #[must_use]
    pub fn with_os_ext_prop(mut self, os_ext_prop: OsExtProp) -> Self {
        self.os_ext_props.push(os_ext_prop);
        self
    }

    /// Adds a custom descriptor after the interface descriptor.
    #[must_use]
    pub fn with_custom_desc(mut self, custom_desc: CustomDesc) -> Self {
        self.custom_descs.push(custom_desc);
        self
    }
}

/// Interface association.
#[derive(Debug, Clone)]
pub struct Association {
    addr: Arc<()>,
    /// Function class.
    pub function_class: Class,
    /// Function name.
    pub name: HashMap<Language, String>,
}

impl PartialEq for Association {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.addr, &other.addr)
            && self.function_class == other.function_class
            && self.name == other.name
    }
}

impl Eq for Association {}

impl Hash for Association {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.addr).hash(state);
    }
}

impl Association {
    /// Creates a new interface association.
    pub fn new(function_class: Class, name: impl AsRef<str>) -> Self {
        Self {
            addr: Arc::new(()),
            function_class,
            name: [(Language::default(), name.as_ref().to_string())].into(),
        }
    }
}

/// Transfer direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    /// From device to host.
    DeviceToHost,
    /// From host to device.
    HostToDevice,
}

/// Endpoint transfer direction.
pub struct EndpointDirection {
    direction: Direction,
    /// Queue length.
    pub queue_len: u32,
    tx: value::Sender<EndpointIo>,
}

impl fmt::Debug for EndpointDirection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("EndpointDirection")
            .field("direction", &self.direction)
            .field("queue_len", &self.queue_len)
            .finish()
    }
}

impl EndpointDirection {
    const DEFAULT_QUEUE_LEN: u32 = 16;

    /// From device to host.
    pub fn device_to_host() -> (EndpointSender, EndpointDirection) {
        let (tx, rx) = value::channel();
        let writer = EndpointSender(rx);
        let this = Self { direction: Direction::DeviceToHost, tx, queue_len: Self::DEFAULT_QUEUE_LEN };
        (writer, this)
    }

    /// From host to device.
    pub fn host_to_device() -> (EndpointReceiver, EndpointDirection) {
        let (tx, rx) = value::channel();
        let reader = EndpointReceiver(rx);
        let this = Self { direction: Direction::HostToDevice, tx, queue_len: Self::DEFAULT_QUEUE_LEN };
        (reader, this)
    }

    /// Sets the queue length.
    #[must_use]
    pub fn with_queue_len(mut self, queue_len: u32) -> Self {
        self.queue_len = queue_len;
        self
    }
}

/// Endpoint synchronization type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncType {
    /// No synchronization.
    NoSync,
    /// Asynchronous.
    Async,
    /// Adaptive.
    Adaptive,
    /// Synchronous.
    Sync,
}

impl SyncType {
    fn to_attributes(self) -> u8 {
        (match self {
            Self::NoSync => 0b00,
            Self::Async => 0b01,
            Self::Adaptive => 0b10,
            Self::Sync => 0b11,
        } << 2)
    }
}

/// Endpoint usage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UsageType {
    /// Data endpoint.
    Data,
    /// Feedback endpoint.
    Feedback,
    /// Implicit feedback data endpoint.
    ImplicitFeedback,
}

impl UsageType {
    fn to_attributes(self) -> u8 {
        (match self {
            Self::Data => 0b00,
            Self::Feedback => 0b01,
            Self::ImplicitFeedback => 0b10,
        } << 4)
    }
}

/// Endpoint transfer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransferType {
    /// Control.
    Control,
    /// Isochronous.
    Isochronous {
        /// Synchronization type.
        sync: SyncType,
        /// Usage type.
        usage: UsageType,
    },
    /// Bulk.
    Bulk,
    /// Interrupt.
    Interrupt,
}

impl TransferType {
    fn to_attributes(self) -> u8 {
        match self {
            Self::Control => 0b00,
            Self::Isochronous { sync, usage } => 0b01 | sync.to_attributes() | usage.to_attributes(),
            Self::Bulk => 0b10,
            Self::Interrupt => 0b11,
        }
    }
}

/// An USB endpoint.
#[derive(Debug)]
#[non_exhaustive]
pub struct Endpoint {
    /// Endpoint transfer direction.
    pub direction: EndpointDirection,
    /// Transfer type.
    pub transfer: TransferType,
    /// Maximum packet size for high speed.
    pub max_packet_size_hs: u16,
    /// Maximum packet size for super speed.
    pub max_packet_size_ss: u16,
    /// Maximum number of packets that the endpoint can send or receive as a part of a burst
    /// for super speed.
    pub max_burst_ss: u8,
    /// Number of bytes per interval for super speed.
    pub bytes_per_interval_ss: u16,
    /// Interval for polling endpoint for data transfers.
    pub interval: u8,
    /// Data for audio endpoints.
    pub audio: Option<EndpointAudio>,
}

/// Extension of USB endpoint for audio.
#[derive(Debug)]
pub struct EndpointAudio {
    /// Refresh.
    pub refresh: u8,
    /// Sync address.
    pub synch_address: u8,
}

impl Endpoint {
    /// Creates a new bulk endpoint.
    pub fn bulk(direction: EndpointDirection) -> Self {
        Self::custom(direction, TransferType::Bulk)
    }

    /// Creates a new custom endpoint.
    pub fn custom(direction: EndpointDirection, transfer: TransferType) -> Self {
        let transfer_direction = direction.direction;
        Self {
            direction,
            transfer,
            max_packet_size_hs: 512,
            max_packet_size_ss: 1024,
            max_burst_ss: 0,
            bytes_per_interval_ss: 0,
            interval: match transfer_direction {
                Direction::DeviceToHost => 0,
                Direction::HostToDevice => 1,
            },
            audio: None,
        }
    }
}

/// Microsoft extended compatibility descriptor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsExtCompat {
    /// Compatible ID string.
    pub compatible_id: [u8; 8],
    /// Sub-compatible ID string.
    pub sub_compatible_id: [u8; 8],
}

/// Creates a new extended compatibility descriptor.
impl OsExtCompat {
    /// Creates a new extended compatibility descriptor.
    pub const fn new(compatible_id: [u8; 8], sub_compatible_id: [u8; 8]) -> Self {
        Self { compatible_id, sub_compatible_id }
    }

    /// Use Microsoft WinUSB driver.
    pub const fn winusb() -> Self {
        Self::new(*b"WINUSB\0\0", [0; 8])
    }
}

/// Microsoft extended property descriptor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsExtProp {
    /// Property name.
    pub name: String,
    /// Property value.
    pub value: OsRegValue,
}

impl OsExtProp {
    /// Creates a new extended property descriptor.
    pub fn new(name: impl AsRef<str>, value: impl Into<OsRegValue>) -> Self {
        Self { name: name.as_ref().to_string(), value: value.into() }
    }

    /// Sets the device interface GUID.
    pub fn device_interface_guid(guid: Uuid) -> Self {
        Self::new("DeviceInterfaceGUID", guid.to_string())
    }

    // Unsupported by Linux 6.5
    //
    //     /// Indicate whether the device can power down when idle (selective suspend).
    //     pub fn device_idle_enabled(enabled: bool) -> Self {
    //         Self::new("DeviceIdleEnabled", u32::from(enabled))
    //     }
    //
    //     /// Indicate whether the device can be suspended when idle by default.
    //     pub fn default_idle_state(state: bool) -> Self {
    //         Self::new("DefaultIdleState", u32::from(state))
    //     }
    //
    //     /// Indicate the amount of time in milliseconds to wait before determining that a device is idle.
    //     pub fn default_idle_timeout(timeout_ms: u32) -> Self {
    //         Self::new("DefaultIdleTimeout", timeout_ms)
    //     }
    //
    //     /// Indicate whether to allow the user to control the ability of the device to enable
    //     /// or disable USB selective suspend.
    //     pub fn user_set_device_idle_enabled(enabled: bool) -> Self {
    //         Self::new("UserSetDeviceIdleEnabled", u32::from(enabled))
    //     }
    //
    //     /// Indicate whether to allow the user to control the ability of the device to wake the system
    //     /// from a low-power state.
    //     pub fn system_wake_enabled(enabled: bool) -> Self {
    //         Self::new("SystemWakeEnabled", u32::from(enabled))
    //     }

    fn as_os_ext_prop(&self) -> ffs::OsExtProp {
        let mut name = self.name.clone();
        name.push('\0');
        ffs::OsExtProp { name, data_type: self.value.as_type(), data: self.value.as_bytes() }
    }
}

/// Microsoft registry value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OsRegValue {
    /// Unicode string.
    Sz(String),
    /// Unicode string that includes environment variables.
    ExpandSz(String),
    /// Free-form binary.
    Binary(Vec<u8>),
    /// Little-endian 32-bit integer.
    DwordLe(u32),
    /// Big-endian 32-bit integer.
    DwordBe(u32),
    /// Unicode string that contains a symbolic link.
    Link(String),
    /// Multiple Unicode strings.
    MultiSz(Vec<String>),
}

impl OsRegValue {
    fn as_type(&self) -> u32 {
        match self {
            Self::Sz(_) => 1,
            Self::ExpandSz(_) => 2,
            Self::Binary(_) => 3,
            Self::DwordLe(_) => 4,
            Self::DwordBe(_) => 5,
            Self::Link(_) => 6,
            Self::MultiSz(_) => 7,
        }
    }

    fn as_bytes(&self) -> Vec<u8> {
        match self {
            Self::Sz(s) => [s.as_bytes(), &[0]].concat().to_vec(),
            Self::ExpandSz(s) => [s.as_bytes(), &[0]].concat().to_vec(),
            Self::Binary(s) => s.clone(),
            Self::DwordLe(v) => v.to_le_bytes().to_vec(),
            Self::DwordBe(v) => v.to_be_bytes().to_vec(),
            Self::Link(s) => [s.as_bytes(), &[0]].concat().to_vec(),
            Self::MultiSz(ss) => ss.iter().flat_map(|s| [s.as_bytes(), &[0]].concat()).collect(),
        }
    }
}

impl From<String> for OsRegValue {
    fn from(value: String) -> Self {
        Self::Sz(value)
    }
}

impl From<&str> for OsRegValue {
    fn from(value: &str) -> Self {
        Self::Sz(value.to_string())
    }
}

impl From<Vec<u8>> for OsRegValue {
    fn from(value: Vec<u8>) -> Self {
        Self::Binary(value)
    }
}

impl From<&[u8]> for OsRegValue {
    fn from(value: &[u8]) -> Self {
        Self::Binary(value.to_vec())
    }
}

impl From<u32> for OsRegValue {
    fn from(value: u32) -> Self {
        Self::DwordLe(value)
    }
}

impl From<Vec<String>> for OsRegValue {
    fn from(value: Vec<String>) -> Self {
        Self::MultiSz(value)
    }
}

/// Builder for custom USB interface, implemented in user code.
#[derive(Debug)]
#[non_exhaustive]
pub struct CustomBuilder {
    /// USB interfaces.
    pub interfaces: Vec<Interface>,
    /// Receive control requests that are not explicitly directed to
    /// an interface or endpoint.
    pub all_ctrl_recipient: bool,
    /// Receive control requests in configuration 0.
    pub config0_setup: bool,
    /// FunctionFS mount directory.
    ///
    /// The parent directory must exist.
    /// If unspecified, a directory starting with `/dev/ffs-*` is created and used.
    pub ffs_dir: Option<PathBuf>,
    /// FunctionFS root permissions.
    pub ffs_root_mode: Option<u32>,
    /// FunctionFS file permissions.
    pub ffs_file_mode: Option<u32>,
    /// FunctionFS user id.
    pub ffs_uid: Option<u32>,
    /// FunctionFS group id.
    pub ffs_gid: Option<u32>,
    /// Do not disconnect USB gadget when interface files are closed.
    pub ffs_no_disconnect: bool,
    /// Do not initialize FunctionFS.
    ///
    /// No FunctionFS files are opened. This must then be done externally.
    pub ffs_no_init: bool,
    /// Do not mount FunctionFS.
    ///
    /// Implies [`ffs_no_init`](Self::ffs_no_init).
    pub ffs_no_mount: bool,
}

impl CustomBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Custom, Handle) {
        let dir = FunctionDir::new();
        let (ep0_tx, ep0_rx) = value::channel();
        let (ffs_dir_tx, ffs_dir_rx) = value::channel();
        let ep_files = Arc::new(Mutex::new(Vec::new()));
        (
            Custom {
                dir: dir.clone(),
                ep0: ep0_rx,
                setup_event: None,
                ep_files: ep_files.clone(),
                existing_ffs: false,
                ffs_dir: ffs_dir_rx,
            },
            Handle::new(CustomFunction {
                builder: self,
                dir,
                ep0_tx,
                ep_files,
                ffs_dir_created: AtomicBool::new(false),
                ffs_dir_tx,
            }),
        )
    }

    /// Use the specified pre-mounted FunctionFS directory.
    ///
    /// Descriptors and strings are written into the `ep0` device file.
    ///
    /// This allows usage of the custom interface functionality when the USB gadget has
    /// been registered externally.
    pub fn existing(mut self, ffs_dir: impl AsRef<Path>) -> Result<Custom> {
        self.ffs_dir = Some(ffs_dir.as_ref().to_path_buf());

        let dir = FunctionDir::new();
        let (ep0_tx, ep0_rx) = value::channel();
        let (ffs_dir_tx, ffs_dir_rx) = value::channel();
        let ep_files = Arc::new(Mutex::new(Vec::new()));

        let func = CustomFunction {
            builder: self,
            dir: dir.clone(),
            ep0_tx,
            ep_files: ep_files.clone(),
            ffs_dir_created: AtomicBool::new(false),
            ffs_dir_tx,
        };
        func.init()?;

        Ok(Custom { dir, ep0: ep0_rx, setup_event: None, ep_files, existing_ffs: true, ffs_dir: ffs_dir_rx })
    }

    /// Add an USB interface.
    #[must_use]
    pub fn with_interface(mut self, interface: Interface) -> Self {
        self.interfaces.push(interface);
        self
    }

    /// Build functionfs descriptors and strings.
    fn ffs_descs(&self) -> Result<(ffs::Descs, ffs::Strings)> {
        let mut strings = ffs::Strings(HashMap::new());
        let mut add_strings = |strs: &HashMap<Language, String>| {
            let all_langs: HashSet<_> = strings.0.keys().chain(strs.keys()).cloned().collect();
            let str_cnt = strings.0.values().next().map(|s| s.len()).unwrap_or_default();
            for lang in all_langs.into_iter() {
                let lang_strs = strings.0.entry(lang).or_insert_with(|| vec![String::new(); str_cnt]);
                lang_strs.push(strs.get(&lang).cloned().unwrap_or_default());
            }
            u8::try_from(str_cnt + 1).map_err(|_| Error::new(ErrorKind::InvalidInput, "too many strings"))
        };

        let mut fs_descrs = Vec::new();
        let mut hs_descrs = Vec::new();
        let mut ss_descrs = Vec::new();
        let mut os_descrs = Vec::new();

        let mut endpoint_num: u8 = 0;

        let mut assocs: HashMap<Association, ffs::InterfaceAssocDesc> = HashMap::new();

        for (interface_number, intf) in self.interfaces.iter().enumerate() {
            let interface_number: u8 = interface_number
                .try_into()
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "too many interfaces"))?;
            let num_endpoints: u8 = intf
                .endpoints
                .len()
                .try_into()
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "too many endpoints"))?;

            let if_desc = ffs::InterfaceDesc {
                interface_number,
                alternate_setting: 0,
                num_endpoints,
                interface_class: intf.interface_class.class,
                interface_sub_class: intf.interface_class.sub_class,
                interface_protocol: intf.interface_class.protocol,
                name_idx: add_strings(&intf.name)?,
            };
            fs_descrs.push(if_desc.clone().into());
            hs_descrs.push(if_desc.clone().into());
            ss_descrs.push(if_desc.clone().into());

            for custom in &intf.custom_descs {
                fs_descrs.push(custom.clone().into());
                hs_descrs.push(custom.clone().into());
                ss_descrs.push(custom.clone().into());
            }

            for ep in &intf.endpoints {
                endpoint_num += 1;
                if endpoint_num >= ffs::DIR_IN {
                    return Err(Error::new(ErrorKind::InvalidInput, "too many endpoints"));
                }

                let ep_desc = ffs::EndpointDesc {
                    endpoint_address: match ep.direction.direction {
                        Direction::DeviceToHost => endpoint_num | ffs::DIR_IN,
                        Direction::HostToDevice => endpoint_num | ffs::DIR_OUT,
                    },
                    attributes: ep.transfer.to_attributes(),
                    max_packet_size: 0,
                    interval: ep.interval,
                    audio: ep
                        .audio
                        .as_ref()
                        .map(|a| ffs::AudioEndpointDesc { refresh: a.refresh, synch_address: a.synch_address }),
                };
                let ss_comp_desc = ffs::SsEndpointComp {
                    max_burst: ep.max_burst_ss,
                    attributes: 0,
                    bytes_per_interval: ep.bytes_per_interval_ss,
                };

                fs_descrs.push(ep_desc.clone().into());
                hs_descrs
                    .push(ffs::EndpointDesc { max_packet_size: ep.max_packet_size_hs, ..ep_desc.clone() }.into());
                ss_descrs
                    .push(ffs::EndpointDesc { max_packet_size: ep.max_packet_size_ss, ..ep_desc.clone() }.into());
                ss_descrs.push(ss_comp_desc.into());
            }

            if let Some(assoc) = &intf.association {
                let iad = match assocs.entry(assoc.clone()) {
                    Entry::Occupied(ocu) => ocu.into_mut(),
                    Entry::Vacant(vac) => vac.insert(ffs::InterfaceAssocDesc {
                        first_interface: interface_number,
                        interface_count: 0,
                        function_class: assoc.function_class.class,
                        function_sub_class: assoc.function_class.sub_class,
                        function_protocol: assoc.function_class.protocol,
                        name_idx: add_strings(&assoc.name)?,
                    }),
                };

                if iad.first_interface + interface_number != interface_number {
                    return Err(Error::new(ErrorKind::InvalidInput, "associated interfaces must be adjacent"));
                }

                iad.interface_count += 1;
            }

            if !intf.os_ext_compat.is_empty() {
                let os_desc = ffs::OsDesc {
                    interface: interface_number,
                    ext: ffs::OsDescExt::ExtCompat(
                        intf.os_ext_compat
                            .iter()
                            .map(|oe| ffs::OsExtCompat {
                                first_interface_number: interface_number,
                                compatible_id: oe.compatible_id,
                                sub_compatible_id: oe.sub_compatible_id,
                            })
                            .collect(),
                    ),
                };
                os_descrs.push(os_desc);
            }

            if !intf.os_ext_props.is_empty() {
                let os_desc = ffs::OsDesc {
                    interface: interface_number,
                    ext: ffs::OsDescExt::ExtProp(
                        intf.os_ext_props.iter().map(|oep| oep.as_os_ext_prop()).collect(),
                    ),
                };
                os_descrs.push(os_desc);
            }
        }

        for iad in assocs.into_values() {
            fs_descrs.push(iad.clone().into());
            hs_descrs.push(iad.clone().into());
            ss_descrs.push(iad.clone().into());
        }

        let mut flags = ffs::Flags::empty();
        flags.set(ffs::Flags::ALL_CTRL_RECIP, self.all_ctrl_recipient);
        flags.set(ffs::Flags::CONFIG0_SETUP, self.config0_setup);

        let descs = ffs::Descs { flags, eventfd: None, fs_descrs, hs_descrs, ss_descrs, os_descrs };
        Ok((descs, strings))
    }

    /// Gets the descriptor and string data for writing into `ep0` of FunctionFS.
    ///
    /// Normally, this is done automatically when the custom function is registered.
    /// This function is only useful when descriptors and strings should be written
    /// to `ep0` by other means.
    pub fn ffs_descriptors_and_strings(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let (descs, strs) = self.ffs_descs()?;
        Ok((descs.to_bytes()?, strs.to_bytes()?))
    }
}

fn default_ffs_dir(instance: &OsStr) -> PathBuf {
    let mut name: OsString = "ffs-".into();
    name.push(instance);
    Path::new("/dev").join(name)
}

#[derive(Debug)]
struct CustomFunction {
    builder: CustomBuilder,
    dir: FunctionDir,
    ep0_tx: value::Sender<Weak<File>>,
    ep_files: Arc<Mutex<Vec<Arc<File>>>>,
    ffs_dir_created: AtomicBool,
    ffs_dir_tx: value::Sender<PathBuf>,
}

impl CustomFunction {
    /// FunctionFS directory.
    fn ffs_dir(&self) -> Result<PathBuf> {
        match &self.builder.ffs_dir {
            Some(ffs_dir) => Ok(ffs_dir.clone()),
            None => Ok(default_ffs_dir(&self.dir.instance()?)),
        }
    }

    /// Initialize FunctionFS.
    ///
    /// It must already be mounted.
    fn init(&self) -> Result<()> {
        let ffs_dir = self.ffs_dir()?;

        if !self.builder.ffs_no_init {
            let (descs, strs) = self.builder.ffs_descs()?;
            log::trace!("functionfs descriptors: {descs:x?}");
            log::trace!("functionfs strings: {strs:?}");

            let ep0_path = ffs_dir.join("ep0");
            let mut ep0 = File::options().read(true).write(true).open(&ep0_path)?;

            log::debug!("writing functionfs descriptors to {}", ep0_path.display());
            let descs_data = descs.to_bytes()?;
            log::trace!("functionfs descriptor data: {descs_data:x?}");
            if ep0.write(&descs_data)? != descs_data.len() {
                return Err(Error::new(ErrorKind::UnexpectedEof, "short descriptor write"));
            }

            log::debug!("writing functionfs strings to {}", ep0_path.display());
            let strs_data = strs.to_bytes()?;
            log::trace!("functionfs strings data: {strs_data:x?}");
            if ep0.write(&strs_data)? != strs_data.len() {
                return Err(Error::new(ErrorKind::UnexpectedEof, "short strings write"));
            }

            log::debug!("functionfs initialized");

            // Open endpoint files.
            let mut endpoint_num = 0;
            let mut ep_files = Vec::new();
            for intf in &self.builder.interfaces {
                for ep in &intf.endpoints {
                    endpoint_num += 1;

                    let ep_path = ffs_dir.join(format!("ep{endpoint_num}"));
                    let (ep_io, ep_file) = EndpointIo::new(ep_path, ep.direction.queue_len)?;
                    ep.direction.tx.send(ep_io).unwrap();
                    ep_files.push(ep_file);
                }
            }

            // Provide endpoint 0 file.
            let ep0 = Arc::new(ep0);
            self.ep0_tx.send(Arc::downgrade(&ep0)).unwrap();
            ep_files.push(ep0);

            *self.ep_files.lock().unwrap() = ep_files;
        }

        self.ffs_dir_tx.send(ffs_dir).unwrap();

        Ok(())
    }

    /// Close all device files.
    fn close(&self) {
        self.ep_files.lock().unwrap().clear();
    }
}

impl Function for CustomFunction {
    fn driver(&self) -> OsString {
        driver().to_os_string()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if self.builder.ffs_no_mount {
            return Ok(());
        }

        let ffs_dir = self.ffs_dir()?;
        log::debug!("creating functionfs directory {}", ffs_dir.display());
        match fs::create_dir(&ffs_dir) {
            Ok(()) => self.ffs_dir_created.store(true, Ordering::SeqCst),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err),
        }

        let mount_opts = ffs::MountOptions {
            no_disconnect: self.builder.ffs_no_disconnect,
            rmode: self.builder.ffs_root_mode,
            fmode: self.builder.ffs_file_mode,
            mode: None,
            uid: self.builder.ffs_uid,
            gid: self.builder.ffs_gid,
        };
        log::debug!("mounting functionfs into {} using options {mount_opts:?}", ffs_dir.display());
        ffs::mount(&self.dir.instance()?, &ffs_dir, &mount_opts)?;

        self.init()
    }

    fn pre_removal(&self) -> Result<()> {
        self.close();
        Ok(())
    }

    fn post_removal(&self, _dir: &Path) -> Result<()> {
        if self.ffs_dir_created.load(Ordering::SeqCst) {
            if let Ok(ffs_dir) = self.ffs_dir() {
                let _ = fs::remove_dir(ffs_dir);
            }
        }
        Ok(())
    }
}

pub(crate) fn remove_handler(dir: PathBuf) -> Result<()> {
    let (_driver, instance) =
        split_function_dir(&dir).ok_or_else(|| Error::new(ErrorKind::InvalidInput, "invalid configfs dir"))?;

    for mount in MountIter::new()? {
        let Ok(mount) = mount else { continue };

        if mount.fstype == ffs::FS_TYPE && mount.source == instance {
            log::debug!("unmounting functionfs {} from {}", instance.to_string_lossy(), mount.dest.display());
            if let Err(err) = ffs::umount(&mount.dest, false) {
                log::debug!("unmount failed, trying lazy unmount: {err}");
                ffs::umount(&mount.dest, true)?;
            }

            if mount.dest == default_ffs_dir(instance) {
                let _ = fs::remove_dir(&mount.dest);
            }
        }
    }

    Ok(())
}

/// Custom USB interface, implemented in user code.
///
/// Dropping this causes all endpoint files to be closed.
/// However, the FunctionFS instance stays mounted until the USB gadget is unregistered.
#[derive(Debug)]
pub struct Custom {
    dir: FunctionDir,
    ep0: value::Receiver<Weak<File>>,
    setup_event: Option<Direction>,
    ep_files: Arc<Mutex<Vec<Arc<File>>>>,
    existing_ffs: bool,
    ffs_dir: value::Receiver<PathBuf>,
}

impl Custom {
    /// Creates a new USB custom function builder.
    pub fn builder() -> CustomBuilder {
        CustomBuilder {
            interfaces: Vec::new(),
            all_ctrl_recipient: false,
            config0_setup: false,
            ffs_dir: None,
            ffs_root_mode: None,
            ffs_file_mode: None,
            ffs_uid: None,
            ffs_gid: None,
            ffs_no_disconnect: false,
            ffs_no_init: false,
            ffs_no_mount: false,
        }
    }

    /// Access to registration status.
    ///
    /// The registration status is not available when [`CustomBuilder::existing`] has been
    /// used to create this object.
    pub fn status(&self) -> Option<Status> {
        if !self.existing_ffs {
            Some(self.dir.status())
        } else {
            None
        }
    }

    fn ep0(&mut self) -> Result<Arc<File>> {
        let ep0 = self.ep0.get()?;
        ep0.upgrade().ok_or_else(|| Error::new(ErrorKind::BrokenPipe, "USB gadget was removed"))
    }

    /// Returns real address of an interface.
    pub fn real_address(&mut self, intf: u8) -> Result<u8> {
        let ep0 = self.ep0()?;
        let address = unsafe { ffs::interface_revmap(ep0.as_raw_fd(), intf.into()) }?;
        Ok(address as u8)
    }

    /// Clear previous event if it was forgotten.
    fn clear_prev_event(&mut self) -> Result<()> {
        let mut ep0 = self.ep0()?;

        let mut buf = [0; 1];
        match self.setup_event.take() {
            Some(Direction::DeviceToHost) => {
                let _ = ep0.read(&mut buf)?;
            }
            Some(Direction::HostToDevice) => {
                let _ = ep0.write(&buf)?;
            }
            None => (),
        }

        Ok(())
    }

    /// Blocking read event.
    fn read_event(&mut self) -> Result<Event> {
        let mut ep0 = self.ep0()?;

        let mut buf = [0; ffs::Event::SIZE];
        let n = ep0.read(&mut buf)?;
        if n != ffs::Event::SIZE {
            return Err(Error::new(ErrorKind::InvalidData, "invalid event size"));
        }
        let raw_event = ffs::Event::parse(&buf)?;
        Ok(Event::from_ffs(raw_event, self))
    }

    /// Wait for an event for the specified duration.
    fn wait_event_sync(&mut self, timeout: Option<Duration>) -> Result<bool> {
        let ep0 = self.ep0()?;

        let mut fds = [PollFd::new(ep0.as_fd(), PollFlags::POLLIN)];
        poll(&mut fds, timeout.map(|d| d.as_millis().try_into().unwrap()).unwrap_or(PollTimeout::NONE))?;
        Ok(fds[0].revents().map(|e| e.contains(PollFlags::POLLIN)).unwrap_or_default())
    }

    /// Asynchronously wait for an event to be available.
    #[cfg(feature = "tokio")]
    pub async fn wait_event(&mut self) -> Result<()> {
        use tokio::io::{unix::AsyncFd, Interest};

        let ep0 = self.ep0()?;

        let async_fd = AsyncFd::with_interest(ep0.as_fd(), Interest::READABLE)?;
        let mut guard = async_fd.readable().await?;
        guard.clear_ready();

        Ok(())
    }

    /// Returns whether events are available for processing.
    pub fn has_event(&mut self) -> bool {
        self.wait_event_sync(Some(Duration::ZERO)).unwrap_or_default()
    }

    /// Wait for an event and returns it.
    ///
    /// Blocks until an event becomes available.
    pub fn event(&mut self) -> Result<Event> {
        self.clear_prev_event()?;
        self.read_event()
    }

    /// Wait for an event with a timeout and returns it.
    ///
    /// Blocks until an event becomes available.
    pub fn event_timeout(&mut self, timeout: Duration) -> Result<Option<Event>> {
        if self.wait_event_sync(Some(timeout))? {
            Ok(Some(self.read_event()?))
        } else {
            Ok(None)
        }
    }

    /// Gets the next event, if available.
    ///
    /// Does not wait for an event to become available.
    pub fn try_event(&mut self) -> Result<Option<Event>> {
        self.clear_prev_event()?;

        if self.has_event() {
            Ok(Some(self.read_event()?))
        } else {
            Ok(None)
        }
    }

    /// File descriptor of endpoint 0.
    pub fn fd(&mut self) -> Result<RawFd> {
        let ep0 = self.ep0()?;
        Ok(ep0.as_raw_fd())
    }

    /// FunctionFS directory.
    pub fn ffs_dir(&mut self) -> Result<PathBuf> {
        Ok(self.ffs_dir.get()?.clone())
    }
}

impl Drop for Custom {
    fn drop(&mut self) {
        self.ep_files.lock().unwrap().clear();
    }
}

/// USB event.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event<'a> {
    /// Bind to gadget.
    Bind,
    /// Unbind from gadget.
    Unbind,
    /// Function was enabled.
    Enable,
    /// Function was disabled.
    Disable,
    /// Device suspend.
    Suspend,
    /// Device resume.
    Resume,
    /// Control request with data from host to device.
    SetupHostToDevice(CtrlReceiver<'a>),
    /// Control request with data from device to host.
    SetupDeviceToHost(CtrlSender<'a>),
    /// Unknown event.
    Unknown(u8),
}

impl<'a> Event<'a> {
    fn from_ffs(raw: ffs::Event, custom: &'a mut Custom) -> Self {
        match raw.event_type {
            ffs::event::BIND => Self::Bind,
            ffs::event::UNBIND => Self::Unbind,
            ffs::event::ENABLE => Self::Enable,
            ffs::event::DISABLE => Self::Disable,
            ffs::event::SUSPEND => Self::Suspend,
            ffs::event::RESUME => Self::Resume,
            ffs::event::SETUP => {
                let ctrl_req = ffs::CtrlReq::parse(&raw.data).unwrap();
                if (ctrl_req.request_type & ffs::DIR_IN) != 0 {
                    custom.setup_event = Some(Direction::DeviceToHost);
                    Self::SetupDeviceToHost(CtrlSender { ctrl_req, custom })
                } else {
                    custom.setup_event = Some(Direction::HostToDevice);
                    Self::SetupHostToDevice(CtrlReceiver { ctrl_req, custom })
                }
            }
            other => Self::Unknown(other),
        }
    }
}

pub use ffs::CtrlReq;

/// Sender for response to USB control request.
///
/// Dropping this stalls the endpoint.
pub struct CtrlSender<'a> {
    ctrl_req: CtrlReq,
    custom: &'a mut Custom,
}

impl fmt::Debug for CtrlSender<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CtrlSender").field("ctrl_req", &self.ctrl_req).finish()
    }
}

impl CtrlSender<'_> {
    /// The control request.
    pub const fn ctrl_req(&self) -> &CtrlReq {
        &self.ctrl_req
    }

    /// Whether no data is associated with this control request.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Length of data expected by host.
    pub fn len(&self) -> usize {
        self.ctrl_req.length.into()
    }

    /// Send the response to the USB host.
    ///
    /// Returns the number of bytes sent.
    pub fn send(self, data: &[u8]) -> Result<usize> {
        let mut file = self.custom.ep0()?;

        let n = file.write(data)?;

        self.custom.setup_event = None;
        Ok(n)
    }

    /// Stall the endpoint.
    pub fn halt(mut self) -> Result<()> {
        self.do_halt()
    }

    fn do_halt(&mut self) -> Result<()> {
        let mut file = self.custom.ep0()?;

        let mut buf = [0; 1];
        let _ = file.read(&mut buf)?;

        self.custom.setup_event = None;
        Ok(())
    }
}

impl Drop for CtrlSender<'_> {
    fn drop(&mut self) {
        if self.custom.setup_event.is_some() {
            let _ = self.do_halt();
        }
    }
}

/// Receiver for data belonging to USB control request.
///
/// Dropping this stalls the endpoint.
pub struct CtrlReceiver<'a> {
    ctrl_req: CtrlReq,
    custom: &'a mut Custom,
}

impl fmt::Debug for CtrlReceiver<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CtrlReceiver").field("ctrl_req", &self.ctrl_req).finish()
    }
}

impl CtrlReceiver<'_> {
    /// The control request.
    pub const fn ctrl_req(&self) -> &CtrlReq {
        &self.ctrl_req
    }

    /// Whether no data is associated with this control request.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Length of data the host wants to send.
    pub fn len(&self) -> usize {
        self.ctrl_req.length.into()
    }

    /// Receive all data from the USB host.
    pub fn recv_all(self) -> Result<Vec<u8>> {
        let mut buf = vec![0; self.len()];
        self.recv(&mut buf)?;
        Ok(buf)
    }

    /// Receive the data from the USB host into the provided buffer.
    ///
    /// Returns the amount of data received.
    pub fn recv(self, data: &mut [u8]) -> Result<usize> {
        let mut file = self.custom.ep0()?;

        let n = file.read(data)?;

        self.custom.setup_event = None;
        Ok(n)
    }

    /// Stall the endpoint.
    pub fn halt(mut self) -> Result<()> {
        self.do_halt()
    }

    fn do_halt(&mut self) -> Result<()> {
        let mut file = self.custom.ep0()?;

        let buf = [0; 1];
        let _ = file.write(&buf)?;

        self.custom.setup_event = None;
        Ok(())
    }
}

impl Drop for CtrlReceiver<'_> {
    fn drop(&mut self) {
        if self.custom.setup_event.is_some() {
            let _ = self.do_halt();
        }
    }
}

/// Endpoint IO access.
struct EndpointIo {
    path: PathBuf,
    file: Weak<File>,
    aio: aio::Driver,
}

impl EndpointIo {
    fn new(path: PathBuf, queue_len: u32) -> Result<(Self, Arc<File>)> {
        log::debug!("opening endpoint file {} with queue length {queue_len}", path.display());
        let file = Arc::new(File::options().read(true).write(true).open(&path)?);
        let aio = aio::Driver::new(queue_len, Some(path.to_string_lossy().to_string()))?;
        Ok((Self { path, file: Arc::downgrade(&file), aio }, file))
    }

    fn file(&self) -> Result<Arc<File>> {
        self.file.upgrade().ok_or_else(|| Error::new(ErrorKind::BrokenPipe, "USB gadget was removed"))
    }
}

impl fmt::Debug for EndpointIo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

impl Drop for EndpointIo {
    fn drop(&mut self) {
        log::debug!("releasing endpoint file {}", self.path.display());
    }
}

/// USB endpoint control interface.
///
/// All control requests are executed immediately, bypassing the send or receive queue.
#[derive(Debug)]
pub struct EndpointControl<'a> {
    io: &'a EndpointIo,
    direction: Direction,
}

pub use ffs::{AudioEndpointDesc as RawAudioEndpointDesc, EndpointDesc as RawEndpointDesc};

impl<'a> EndpointControl<'a> {
    fn new(io: &'a EndpointIo, direction: Direction) -> Self {
        Self { io, direction }
    }

    /// Returns how many bytes are "unclaimed" in the endpoint FIFO.
    ///
    /// Might be useful for precise fault handling, when the hardware allows it.
    ///
    /// Device-to-host transfers may be reported to the gadget driver as complete
    /// when the FIFO is loaded, before the host reads the data.
    ///
    /// Host-to-device transfers may be reported to the host's "client" driver as
    /// complete when they're sitting in the FIFO unread.
    pub fn unclaimed_fifo(&self) -> Result<usize> {
        let file = self.io.file()?;
        let bytes = unsafe { ffs::fifo_status(file.as_raw_fd()) }?;
        Ok(bytes as usize)
    }

    /// Discards any unclaimed data in the endpoint FIFO.
    pub fn discard_fifo(&self) -> Result<()> {
        let file = self.io.file()?;
        unsafe { ffs::fifo_flush(file.as_raw_fd()) }?;
        Ok(())
    }

    /// Sets the endpoint halt feature.
    ///
    /// Use this to stall an endpoint, perhaps as an error report.
    /// The endpoint stays halted (will not stream any data) until the host
    /// clears this feature.
    /// Empty the endpoint's request queue first, to make sure no
    /// inappropriate transfers happen.
    pub fn halt(&self) -> Result<()> {
        let mut file = self.io.file()?;
        let mut buf = [0; 1];
        match self.direction {
            Direction::DeviceToHost => {
                let _ = file.read(&mut buf)?;
            }
            Direction::HostToDevice => {
                let _ = file.write(&buf)?;
            }
        }
        Ok(())
    }

    /// Clears endpoint halt, and resets toggle.
    ///
    /// Use this when responding to the standard USB "set interface" request,
    /// for endpoints that are not reconfigured, after clearing any other state
    /// in the endpoint's IO queue.
    pub fn clear_halt(&self) -> Result<()> {
        let file = self.io.file()?;
        unsafe { ffs::clear_halt(file.as_raw_fd()) }?;
        Ok(())
    }

    /// Returns real `bEndpointAddress` of the endpoint.
    pub fn real_address(&self) -> Result<u8> {
        let file = self.io.file()?;
        let address = unsafe { ffs::endpoint_revmap(file.as_raw_fd()) }?;
        Ok(address as u8)
    }

    /// Returns the endpoint descriptor in-use.
    pub fn descriptor(&self) -> Result<RawEndpointDesc> {
        let file = self.io.file()?;
        let mut data = [0; ffs::EndpointDesc::AUDIO_SIZE];
        unsafe { ffs::endpoint_desc(file.as_raw_fd(), &mut data) }?;
        ffs::EndpointDesc::parse(&data)
    }

    /// File descriptor of this endpoint.
    pub fn fd(&mut self) -> Result<RawFd> {
        let file = self.io.file()?;
        Ok(file.as_raw_fd())
    }
}

/// USB endpoint from device to host sender.
#[derive(Debug)]
pub struct EndpointSender(value::Receiver<EndpointIo>);

impl EndpointSender {
    /// Gets the endpoint control interface.
    pub fn control(&mut self) -> Result<EndpointControl> {
        let io = self.0.get()?;
        Ok(EndpointControl::new(io, Direction::DeviceToHost))
    }

    /// Maximum packet size.
    pub fn max_packet_size(&mut self) -> Result<usize> {
        Ok(self.control()?.descriptor()?.max_packet_size.into())
    }

    /// Send data synchronously.
    ///
    /// Blocks until the send operation completes and returns its result.
    pub fn send_and_flush(&mut self, data: Bytes) -> Result<()> {
        self.send(data)?;
        self.flush()
    }

    /// Send data synchronously with a timeout.
    ///
    /// Blocks until the send operation completes and returns its result.
    pub fn send_and_flush_timeout(&mut self, data: Bytes, timeout: Duration) -> Result<()> {
        self.send(data)?;

        let res = self.flush_timeout(timeout);
        if res.is_err() {
            self.cancel()?;
        }
        res
    }

    /// Enqueue data for sending.
    ///
    /// Blocks until send space is available.
    /// Also returns errors of previously enqueued send operations.
    pub fn send(&mut self, data: Bytes) -> Result<()> {
        self.ready()?;
        self.try_send(data)
    }

    /// Asynchronously Enqueue data for sending.
    ///
    /// Waits until send space is available.
    /// Also returns errors of previously enqueued send operations.
    #[cfg(feature = "tokio")]
    pub async fn send_async(&mut self, data: Bytes) -> Result<()> {
        self.wait_ready().await?;
        self.try_send(data)
    }

    /// Enqueue data for sending with a timeout.
    ///
    /// Blocks until send space is available with the specified timeout.
    /// Also returns errors of previously enqueued send operations.
    pub fn send_timeout(&mut self, data: Bytes, timeout: Duration) -> Result<()> {
        self.ready_timeout(timeout)?;
        self.try_send(data)
    }

    /// Enqueue data for sending without waiting for send space.
    ///
    /// Fails if no send space is available.
    /// Also returns errors of previously enqueued send operations.
    pub fn try_send(&mut self, data: Bytes) -> Result<()> {
        self.try_ready()?;

        let io = self.0.get()?;
        let file = io.file()?;
        io.aio.submit(aio::opcode::PWRITE, file.as_raw_fd(), data)?;
        Ok(())
    }

    /// Whether send space is available.
    ///
    /// Send space will only become available when [`ready`](Self::ready),
    /// [`ready_timeout`](Self::ready_timeout) or [`try_ready`](Self::try_ready) are called.
    pub fn is_ready(&mut self) -> bool {
        let Ok(io) = self.0.get() else { return false };
        !io.aio.is_full()
    }

    /// Whether the send queue is empty.
    ///
    /// The send queue will only be drained when [`ready`](Self::ready),
    /// [`ready_timeout`](Self::ready_timeout) or [`try_ready`](Self::try_ready) are called.
    pub fn is_empty(&mut self) -> bool {
        let Ok(io) = self.0.get() else { return true };
        io.aio.is_empty()
    }

    /// Asynchronously wait for send space to be available.
    ///
    /// Also returns errors of previously enqueued send operations.
    #[cfg(feature = "tokio")]
    pub async fn wait_ready(&mut self) -> Result<()> {
        let io = self.0.get()?;

        while io.aio.is_full() {
            let comp = io.aio.wait_completed().await.unwrap();
            comp.result()?;
        }

        Ok(())
    }

    /// Wait for send space to be available.
    ///
    /// Also returns errors of previously enqueued send operations.
    pub fn ready(&mut self) -> Result<()> {
        let io = self.0.get()?;

        while io.aio.is_full() {
            let comp = io.aio.completed().unwrap();
            comp.result()?;
        }

        Ok(())
    }

    /// Wait for send space to be available with a timeout.
    ///
    /// Also returns errors of previously enqueued send operations.
    pub fn ready_timeout(&mut self, timeout: Duration) -> Result<()> {
        let io = self.0.get()?;

        while io.aio.is_full() {
            let comp = io
                .aio
                .completed_timeout(timeout)
                .ok_or_else(|| Error::new(ErrorKind::TimedOut, "timeout waiting for send space"))?;
            comp.result()?;
        }

        Ok(())
    }

    /// Check for availability of send space.
    ///
    /// Also returns errors of previously enqueued send operations.
    pub fn try_ready(&mut self) -> Result<()> {
        let io = self.0.get()?;

        while let Some(comp) = io.aio.try_completed() {
            comp.result()?;
        }

        Ok(())
    }

    /// Waits for all enqueued data to be sent.
    ///
    /// Returns an error if any enqueued send operation has failed.
    pub fn flush(&mut self) -> Result<()> {
        let io = self.0.get()?;

        while let Some(comp) = io.aio.completed() {
            comp.result()?;
        }

        Ok(())
    }

    /// Waits for all enqueued data to be sent.
    ///
    /// Returns an error if any enqueued send operation has failed.
    #[cfg(feature = "tokio")]
    pub async fn flush_async(&mut self) -> Result<()> {
        let io = self.0.get()?;

        while let Some(comp) = io.aio.wait_completed().await {
            comp.result()?;
        }

        Ok(())
    }

    /// Waits for all enqueued data to be sent with a timeout.
    ///
    /// Returns an error if any enqueued send operation has failed.
    pub fn flush_timeout(&mut self, timeout: Duration) -> Result<()> {
        let io = self.0.get()?;

        while let Some(comp) = io.aio.completed_timeout(timeout) {
            comp.result()?;
        }

        if io.aio.is_empty() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::TimedOut, "timeout waiting for send to complete"))
        }
    }

    /// Removes all data from the send queue and clears all errors.
    pub fn cancel(&mut self) -> Result<()> {
        let io = self.0.get()?;

        io.aio.cancel_all();
        while io.aio.completed().is_some() {}

        Ok(())
    }
}

/// USB endpoint from host to device receiver.
#[derive(Debug)]
pub struct EndpointReceiver(value::Receiver<EndpointIo>);

impl EndpointReceiver {
    /// Gets the endpoint control interface.
    pub fn control(&mut self) -> Result<EndpointControl> {
        let io = self.0.get()?;
        Ok(EndpointControl::new(io, Direction::HostToDevice))
    }

    /// Maximum packet size.
    pub fn max_packet_size(&mut self) -> Result<usize> {
        Ok(self.control()?.descriptor()?.max_packet_size.into())
    }

    /// Receive data synchronously.
    ///
    /// The buffer should have been allocated with the desired capacity using [`BytesMut::with_capacity`].
    ///
    /// Blocks until the operation completes and returns its result.
    pub fn recv_and_fetch(&mut self, buf: BytesMut) -> Result<BytesMut> {
        self.try_recv(buf)?;
        Ok(self.fetch()?.unwrap())
    }

    /// Receive data synchronously with a timeout.
    ///
    /// The buffer should have been allocated with the desired capacity using [`BytesMut::with_capacity`].
    ///
    /// Blocks until the operation completes and returns its result.
    pub fn recv_and_fetch_timeout(&mut self, buf: BytesMut, timeout: Duration) -> Result<BytesMut> {
        self.try_recv(buf)?;

        let res = self.fetch_timeout(timeout);
        match res {
            Ok(data) => Ok(data.unwrap()),
            Err(err) => {
                self.cancel()?;
                Err(err)
            }
        }
    }

    /// Receive data.
    ///
    /// The buffer should have been allocated with the desired capacity using [`BytesMut::with_capacity`].
    ///
    /// Waits for space in the receive queue and enqueues the buffer for receiving data.
    /// Returns received data, if a buffer in the receive queue was filled.
    pub fn recv(&mut self, buf: BytesMut) -> Result<Option<BytesMut>> {
        let data = if self.is_ready() { self.try_fetch()? } else { self.fetch()? };
        self.try_recv(buf)?;
        Ok(data)
    }

    /// Asynchronously receive data.
    ///
    /// The buffer should have been allocated with the desired capacity using [`BytesMut::with_capacity`].
    ///
    /// Waits for space in the receive queue and enqueues the buffer for receiving data.
    /// Returns received data, if a buffer in the receive queue was filled.
    #[cfg(feature = "tokio")]
    pub async fn recv_async(&mut self, buf: BytesMut) -> Result<Option<BytesMut>> {
        let data = if self.is_ready() { self.try_fetch()? } else { self.fetch_async().await? };
        self.try_recv(buf)?;
        Ok(data)
    }

    /// Receive data with a timeout.
    ///
    /// The buffer should have been allocated with the desired capacity using [`BytesMut::with_capacity`].
    ///
    /// Waits for space in the receive queue and enqueues the buffer for receiving data.
    /// Returns received data, if a buffer in the receive queue was filled.
    pub fn recv_timeout(&mut self, buf: BytesMut, timeout: Duration) -> Result<Option<BytesMut>> {
        let data = if self.is_ready() { self.try_fetch()? } else { self.fetch_timeout(timeout)? };
        if self.is_ready() {
            self.try_recv(buf)?;
        }
        Ok(data)
    }

    /// Enqueue the buffer for receiving without waiting for receive queue space.
    ///
    /// The buffer should have been allocated with the desired capacity using [`BytesMut::with_capacity`].
    ///
    /// Fails if no receive queue space is available.
    pub fn try_recv(&mut self, buf: BytesMut) -> Result<()> {
        let io = self.0.get()?;
        let file = io.file()?;
        io.aio.submit(aio::opcode::PREAD, file.as_raw_fd(), buf)?;
        Ok(())
    }

    /// Whether receive queue space is available.
    ///
    /// Receive space will only become available when [`fetch`](Self::fetch),
    /// [`fetch_timeout`](Self::fetch_timeout) or [`try_fetch`](Self::try_fetch) are called.
    pub fn is_ready(&mut self) -> bool {
        let Ok(io) = self.0.get() else { return false };
        !io.aio.is_full()
    }

    /// Whether no buffers are enqueued for receiving data.
    ///
    /// The receive queue will only be drained when [`fetch`](Self::fetch),
    /// [`fetch_timeout`](Self::fetch_timeout) or [`try_fetch`](Self::try_fetch) are called.
    pub fn is_empty(&mut self) -> bool {
        let Ok(io) = self.0.get() else { return true };
        io.aio.is_empty()
    }

    /// Waits for data to be received into a previously enqueued receive buffer, then returns it.
    ///
    /// `Ok(None)` is returned if no receive buffers are enqueued.
    pub fn fetch(&mut self) -> Result<Option<BytesMut>> {
        let io = self.0.get()?;

        let Some(comp) = io.aio.completed() else {
            return Ok(None);
        };

        Ok(Some(comp.result()?.try_into().unwrap()))
    }

    /// Asynchronously waits for data to be received into a previously enqueued receive buffer, then returns it.
    ///
    /// `Ok(None)` is returned if no receive buffers are enqueued.
    #[cfg(feature = "tokio")]
    pub async fn fetch_async(&mut self) -> Result<Option<BytesMut>> {
        let io = self.0.get()?;

        let Some(comp) = io.aio.wait_completed().await else {
            return Ok(None);
        };

        Ok(Some(comp.result()?.try_into().unwrap()))
    }

    /// Waits for data to be received into a previously enqueued receive buffer with a timeout,
    /// then returns it.
    ///
    /// `Ok(None)` is returned if no receive buffers are enqueued.
    pub fn fetch_timeout(&mut self, timeout: Duration) -> Result<Option<BytesMut>> {
        let io = self.0.get()?;

        let Some(comp) = io.aio.completed_timeout(timeout) else {
            return Ok(None);
        };

        Ok(Some(comp.result()?.try_into().unwrap()))
    }

    /// If data has been received into a previously enqueued receive buffer, returns it.
    ///
    /// Does not wait for data to be received.
    pub fn try_fetch(&mut self) -> Result<Option<BytesMut>> {
        let io = self.0.get()?;

        let Some(comp) = io.aio.try_completed() else { return Ok(None) };
        let data = comp.result()?;

        Ok(Some(data.try_into().unwrap()))
    }

    /// Removes all buffers from the receive queue and clears all errors.
    pub fn cancel(&mut self) -> Result<()> {
        let io = self.0.get()?;

        io.aio.cancel_all();
        while io.aio.completed().is_some() {}

        Ok(())
    }
}
