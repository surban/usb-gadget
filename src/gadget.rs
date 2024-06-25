//! USB gadget.

use nix::errno::Errno;
use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{Error, ErrorKind, Result},
    os::unix::{
        fs::symlink,
        prelude::{OsStrExt, OsStringExt},
    },
    path::{Path, PathBuf},
};

use crate::{
    configfs_dir, function,
    function::{
        util::{call_remove_handler, init_remove_handlers},
        Handle,
    },
    hex_u16, hex_u8,
    lang::Language,
    request_module, trim_os_str,
    udc::Udc,
    Speed,
};

/// USB gadget or interface class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Class {
    /// Class code.
    pub class: u8,
    /// Subclass code.
    pub sub_class: u8,
    /// Protocol code.
    pub protocol: u8,
}

impl Class {
    /// Vendor specific class code.
    pub const VENDOR_SPECIFIC: u8 = 0xff;

    /// Creates a new USB device or interface class.
    pub const fn new(class: u8, sub_class: u8, protocol: u8) -> Self {
        Self { class, sub_class, protocol }
    }

    /// Creates a new USB device or interface class with vendor-specific class code.
    pub const fn vendor_specific(sub_class: u8, protocol: u8) -> Self {
        Self::new(Self::VENDOR_SPECIFIC, sub_class, protocol)
    }

    /// Indicates that class information should be determined from the interface descriptors in the device.
    ///
    /// Can only be used as device class.
    pub const fn interface_specific() -> Self {
        Self::new(0, 0, 0)
    }
}

/// USB gadget id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id {
    /// Vendor id.
    pub vendor: u16,
    /// Product id.
    pub product: u16,
}

impl Id {
    /// Creates a new USB device id.
    pub const fn new(vendor: u16, product: u16) -> Self {
        Self { vendor, product }
    }
}

/// USB gadget description strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Strings {
    /// Manufacturer name.
    pub manufacturer: String,
    /// Product name.
    pub product: String,
    /// Serial number.
    pub serial_number: String,
}

impl Strings {
    /// Creates new USB device strings.
    pub fn new(manufacturer: impl AsRef<str>, product: impl AsRef<str>, serial_number: impl AsRef<str>) -> Self {
        Self {
            manufacturer: manufacturer.as_ref().to_string(),
            product: product.as_ref().to_string(),
            serial_number: serial_number.as_ref().to_string(),
        }
    }
}

/// USB gadget operating system descriptor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsDescriptor {
    /// Vendor code for requests.
    pub vendor_code: u8,
    /// Signature.
    pub qw_sign: String,
    /// Index of configuration in [`Gadget::configs`] to be reported at index 0.
    ///
    /// Hosts which expect the "OS Descriptors" ask only for configurations at index 0,
    /// but Linux-based USB devices can provide more than one configuration.
    pub config: usize,
}

impl OsDescriptor {
    /// Creates a new instance.
    pub const fn new(vendor_code: u8, qw_sign: String) -> Self {
        Self { vendor_code, qw_sign, config: 0 }
    }

    /// The Microsoft OS descriptor.
    ///
    /// Uses vendor code 0xf0 for requests.
    pub fn microsoft() -> Self {
        Self { vendor_code: 0xf0, qw_sign: "MSFT100".to_string(), config: 0 }
    }
}

/// WebUSB version.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WebUsbVersion {
    /// Version 1.0
    #[default]
    V10,
    /// Other version in BCD format.
    Other(u16),
}

impl From<WebUsbVersion> for u16 {
    fn from(value: WebUsbVersion) -> Self {
        match value {
            WebUsbVersion::V10 => 0x0100,
            WebUsbVersion::Other(ver) => ver,
        }
    }
}

/// USB gadget WebUSB descriptor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WebUsb {
    /// WebUSB specification version number.
    pub version: WebUsbVersion,
    /// bRequest value used for issuing WebUSB requests.
    pub vendor_code: u8,
    /// URL of the device's landing page.
    pub landing_page: String,
}

impl WebUsb {
    /// Creates a new instance.
    pub fn new(vendor_code: u8, landing_page: impl AsRef<str>) -> Self {
        Self { version: WebUsbVersion::default(), vendor_code, landing_page: landing_page.as_ref().to_string() }
    }
}

/// USB gadget configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Maximum power in mA.
    pub max_power: u16,
    /// Self powered?
    pub self_powered: bool,
    /// Remote wakeup?
    pub remote_wakeup: bool,
    /// Configuration description string.
    pub description: HashMap<Language, String>,
    /// Functions, i.e. USB interfaces, present in this configuration.
    pub functions: HashSet<function::Handle>,
}

impl Config {
    /// Creates a new USB gadget configuration.
    pub fn new(description: impl AsRef<str>) -> Self {
        Self {
            max_power: 500,
            self_powered: false,
            remote_wakeup: false,
            description: [(Language::default(), description.as_ref().to_string())].into(),
            functions: Default::default(),
        }
    }

    /// Sets the maximum power in mA.
    #[deprecated(since = "0.7.1", note = "use the field Config::max_power instead")]
    pub fn set_max_power_ma(&mut self, ma: u16) -> Result<()> {
        self.max_power = ma;
        Ok(())
    }

    /// Adds a USB function (interface) to this configuration.
    pub fn add_function(&mut self, function_handle: function::Handle) {
        self.functions.insert(function_handle);
    }

    /// Adds a USB function (interface) to this configuration.
    #[must_use]
    pub fn with_function(mut self, function_handle: function::Handle) -> Self {
        self.add_function(function_handle);
        self
    }

    fn register(
        &self, gadget_dir: &Path, idx: usize, func_dirs: &HashMap<function::Handle, PathBuf>,
    ) -> Result<PathBuf> {
        let dir = gadget_dir.join("configs").join(format!("c.{idx}"));
        log::debug!("creating config at {}", dir.display());
        fs::create_dir(&dir)?;

        let mut attributes = 1 << 7;
        if self.self_powered {
            attributes |= 1 << 6;
        }
        if self.remote_wakeup {
            attributes |= 1 << 5;
        }

        fs::write(dir.join("bmAttributes"), hex_u8(attributes))?;
        fs::write(dir.join("MaxPower"), self.max_power.to_string())?;

        for (&lang, desc) in &self.description {
            let lang_dir = dir.join("strings").join(hex_u16(lang.into()));
            fs::create_dir(&lang_dir)?;
            fs::write(lang_dir.join("configuration"), desc)?;
        }

        for func in &self.functions {
            let func_dir = &func_dirs[func];
            log::debug!("adding function {}", func_dir.display());
            symlink(func_dir, dir.join(func_dir.file_name().unwrap()))?;
        }

        Ok(dir)
    }
}

/// USB version.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UsbVersion {
    /// USB 1.1
    V11,
    /// USB 2.0
    #[default]
    V20,
    /// USB 3.0
    V30,
    /// USB 3.1
    V31,
    /// Other version in BCD format.
    Other(u16),
}

impl From<UsbVersion> for u16 {
    fn from(value: UsbVersion) -> Self {
        match value {
            UsbVersion::V11 => 0x0110,
            UsbVersion::V20 => 0x0200,
            UsbVersion::V30 => 0x0300,
            UsbVersion::V31 => 0x0310,
            UsbVersion::Other(ver) => ver,
        }
    }
}

/// USB gadget definition.
///
/// Fields set to `None` are left at their kernel-provided default values.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Gadget {
    /// USB device class.
    pub device_class: Class,
    /// USB device id.
    pub id: Id,
    /// USB device strings.
    pub strings: HashMap<Language, Strings>,
    /// Maximum endpoint 0 packet size.
    pub max_packet_size0: u8,
    /// Device release number in BCD format.
    ///
    /// No hexadecimal digit must exceed 9.
    pub device_release: u16,
    /// USB specification version.
    pub usb_version: UsbVersion,
    /// Maximum speed supported by driver.
    pub max_speed: Option<Speed>,
    /// OS descriptor extension.
    pub os_descriptor: Option<OsDescriptor>,
    /// WebUSB extension.
    pub web_usb: Option<WebUsb>,
    /// USB device configurations.
    pub configs: Vec<Config>,
}

impl Gadget {
    /// Creates a new USB gadget definition.
    pub fn new(device_class: Class, id: Id, strings: Strings) -> Self {
        Self {
            device_class,
            id,
            strings: [(Language::default(), strings)].into(),
            max_packet_size0: 64,
            device_release: 0x0000,
            usb_version: UsbVersion::default(),
            max_speed: None,
            os_descriptor: None,
            web_usb: None,
            configs: Vec::new(),
        }
    }

    /// Adds a USB device configuration.
    pub fn add_config(&mut self, config: Config) {
        self.configs.push(config);
    }

    /// Adds a USB device configuration.
    #[must_use]
    pub fn with_config(mut self, config: Config) -> Self {
        self.add_config(config);
        self
    }

    /// Sets the OS descriptor.
    #[must_use]
    pub fn with_os_descriptor(mut self, os_descriptor: OsDescriptor) -> Self {
        self.os_descriptor = Some(os_descriptor);
        self
    }

    /// Sets the WebUSB extension.
    #[must_use]
    pub fn with_web_usb(mut self, web_usb: WebUsb) -> Self {
        self.web_usb = Some(web_usb);
        self
    }

    /// Register the USB gadget.
    ///
    /// At least one [configuration](Config) must be added before the gadget
    /// can be registered.
    pub fn register(self) -> Result<RegGadget> {
        if self.configs.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "USB gadget must have at least one configuration"));
        }

        let usb_gadget_dir = usb_gadget_dir()?;

        let mut gadget_idx: u16 = 0;
        let dir = loop {
            let dir = usb_gadget_dir.join(format!("usb-gadget{gadget_idx}"));
            match fs::create_dir(&dir) {
                Ok(()) => break dir,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
                Err(err) => return Err(err),
            }
            gadget_idx = gadget_idx
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorKind::OutOfMemory, "USB gadgets exhausted"))?;
        };

        log::debug!("registering gadget at {}", dir.display());

        fs::write(dir.join("bDeviceClass"), hex_u8(self.device_class.class))?;
        fs::write(dir.join("bDeviceSubClass"), hex_u8(self.device_class.sub_class))?;
        fs::write(dir.join("bDeviceProtocol"), hex_u8(self.device_class.protocol))?;

        fs::write(dir.join("idVendor"), hex_u16(self.id.vendor))?;
        fs::write(dir.join("idProduct"), hex_u16(self.id.product))?;

        fs::write(dir.join("bMaxPacketSize0"), hex_u8(self.max_packet_size0))?;
        fs::write(dir.join("bcdDevice"), hex_u16(self.device_release))?;
        fs::write(dir.join("bcdUSB"), hex_u16(self.usb_version.into()))?;

        if let Some(v) = self.max_speed {
            fs::write(dir.join("max_speed"), v.to_string())?;
        }

        if let Some(webusb) = &self.web_usb {
            let webusb_dir = dir.join("webusb");
            if webusb_dir.is_dir() {
                fs::write(webusb_dir.join("bVendorCode"), hex_u8(webusb.vendor_code))?;
                fs::write(webusb_dir.join("bcdVersion"), hex_u16(webusb.version.into()))?;
                fs::write(webusb_dir.join("landingPage"), &webusb.landing_page)?;
                fs::write(webusb_dir.join("use"), "1")?;
            } else {
                log::warn!("WebUSB descriptor is unsupported by kernel");
            }
        }

        for (&lang, strs) in &self.strings {
            let lang_dir = dir.join("strings").join(hex_u16(lang.into()));
            fs::create_dir(&lang_dir)?;

            fs::write(lang_dir.join("manufacturer"), &strs.manufacturer)?;
            fs::write(lang_dir.join("product"), &strs.product)?;
            fs::write(lang_dir.join("serialnumber"), &strs.serial_number)?;
        }

        let functions: HashSet<_> = self.configs.iter().flat_map(|c| &c.functions).collect();
        let mut func_dirs = HashMap::new();
        for (func_idx, &func) in functions.iter().enumerate() {
            let func_dir = dir.join(
                dir.join("functions")
                    .join(format!("{}.usb-gadget{gadget_idx}-{func_idx}", func.get().driver().to_str().unwrap())),
            );
            log::debug!("creating function at {}", func_dir.display());
            fs::create_dir(&func_dir)?;

            func.get().dir().set_dir(&func_dir);
            func.get().register()?;

            func_dirs.insert(func.clone(), func_dir);
        }

        let mut config_dirs = Vec::new();
        for (idx, config) in self.configs.iter().enumerate() {
            let dir = config.register(&dir, idx + 1, &func_dirs)?;
            config_dirs.push(dir);
        }

        if let Some(os_desc) = &self.os_descriptor {
            let os_desc_dir = dir.join("os_desc");
            if os_desc_dir.is_dir() {
                fs::write(os_desc_dir.join("b_vendor_code"), hex_u8(os_desc.vendor_code))?;
                fs::write(os_desc_dir.join("qw_sign"), &os_desc.qw_sign)?;
                fs::write(os_desc_dir.join("use"), "1")?;

                let config_dir = config_dirs.get(os_desc.config).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "invalid configuration index in OS descriptor")
                })?;
                symlink(config_dir, os_desc_dir.join(config_dir.file_name().unwrap()))?;
            } else {
                log::warn!("USB OS descriptor is unsupported by kernel");
            }
        }

        log::debug!("gadget at {} registered", dir.display());
        Ok(RegGadget { dir, attached: true, func_dirs })
    }

    /// Register and bind USB gadget to a USB device controller (UDC).
    ///
    /// At least one [configuration](Config) must be added before the gadget
    /// can be bound.
    pub fn bind(self, udc: &Udc) -> Result<RegGadget> {
        let reg = self.register()?;
        reg.bind(Some(udc))?;
        Ok(reg)
    }
}

/// USB gadget registered with the system.
///
/// If this was obtained by calling [`Gadget::bind`], the USB gadget will be
/// unbound and removed when this is dropped.
///
/// Call [`registered`] to obtain all gadgets registered on the system.
#[must_use = "The USB gadget is removed when RegGadget is dropped."]
pub struct RegGadget {
    dir: PathBuf,
    attached: bool,
    func_dirs: HashMap<Handle, PathBuf>,
}

impl fmt::Debug for RegGadget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RegGadget").field("name", &self.name()).field("is_attached", &self.is_attached()).finish()
    }
}

impl RegGadget {
    /// Name of this USB gadget in configfs.
    pub fn name(&self) -> &OsStr {
        self.dir.file_name().unwrap()
    }

    /// Path of this USB gadget in configfs.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// If true, the USB gadget will be removed when this is dropped.
    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// The name of the USB device controller (UDC) this gadget is bound to.
    pub fn udc(&self) -> Result<Option<OsString>> {
        let data = OsString::from_vec(fs::read(self.dir.join("UDC"))?);
        let data = trim_os_str(&data);
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(data.to_os_string()))
        }
    }

    /// Binds the gadget to the specified USB device controller (UDC).
    ///
    /// If `udc` is `None`, the gadget is unbound from any UDC.
    pub fn bind(&self, udc: Option<&Udc>) -> Result<()> {
        log::debug!("binding gadget {:?} to {:?}", self, &udc);

        let name = match udc {
            Some(udc) => udc.name().to_os_string(),
            None => "\n".into(),
        };

        match fs::write(self.dir.join("UDC"), name.as_bytes()) {
            Ok(()) => (),
            Err(err) if udc.is_none() && err.raw_os_error() == Some(Errno::ENODEV as i32) => (),
            Err(err) => return Err(err),
        }

        for func in self.func_dirs.keys() {
            func.get().dir().set_bound(udc.is_some());
        }

        Ok(())
    }

    /// Detach the handle from the USB gadget while keeping the USB gadget active.
    pub fn detach(&mut self) {
        self.attached = false;
    }

    fn do_remove(&mut self) -> Result<()> {
        for func in self.func_dirs.keys() {
            func.get().pre_removal()?;
        }

        for func in self.func_dirs.keys() {
            func.get().dir().set_bound(false);
        }

        remove_at(&self.dir)?;

        for func in self.func_dirs.keys() {
            func.get().dir().reset_dir();
        }

        for (func, dir) in &self.func_dirs {
            func.get().post_removal(dir)?;
        }

        self.detach();
        Ok(())
    }

    /// Unbind from the UDC and remove the USB gadget.
    pub fn remove(mut self) -> Result<()> {
        self.do_remove()
    }
}

impl Drop for RegGadget {
    fn drop(&mut self) {
        if self.attached {
            if let Err(err) = self.do_remove() {
                log::warn!("removing gadget at {} failed: {err}", self.dir.display());
            }
        }
    }
}

/// Remove USB gadget at specified configfs gadget directory.
fn remove_at(dir: &Path) -> Result<()> {
    log::debug!("removing gadget at {}", dir.display());

    init_remove_handlers();

    let _ = fs::write(dir.join("UDC"), "\n");

    if let Ok(entries) = fs::read_dir(dir.join("os_desc")) {
        for file in entries {
            let Ok(file) = file else { continue };
            let Ok(file_type) = file.file_type() else { continue };
            if file_type.is_symlink() {
                fs::remove_file(file.path())?;
            }
        }
    }

    for config_dir in fs::read_dir(dir.join("configs"))? {
        let Ok(config_dir) = config_dir else { continue };
        if !config_dir.metadata()?.is_dir() {
            continue;
        }

        for func in fs::read_dir(config_dir.path())? {
            let Ok(func) = func else { continue };
            if func.metadata()?.is_symlink() {
                fs::remove_file(func.path())?;
            }
        }

        for lang in fs::read_dir(config_dir.path().join("strings"))? {
            let Ok(lang) = lang else { continue };
            if lang.metadata()?.is_dir() {
                fs::remove_dir(lang.path())?;
            }
        }

        fs::remove_dir(config_dir.path())?;
    }

    for func_dir in fs::read_dir(dir.join("functions"))? {
        let Ok(func_dir) = func_dir else { continue };
        if !func_dir.metadata()?.is_dir() {
            continue;
        }

        call_remove_handler(&func_dir.path())?;

        fs::remove_dir(func_dir.path())?;
    }

    for lang in fs::read_dir(dir.join("strings"))? {
        let Ok(lang) = lang else { continue };
        if lang.metadata()?.is_dir() {
            fs::remove_dir(lang.path())?;
        }
    }

    fs::remove_dir(dir)?;

    log::debug!("removed gadget at {}", dir.display());
    Ok(())
}

/// The path to the USB gadget configuration directory within configfs.
fn usb_gadget_dir() -> Result<PathBuf> {
    let _ = request_module("libcomposite");

    let usb_gadget_dir = configfs_dir()?.join("usb_gadget");
    if usb_gadget_dir.is_dir() {
        Ok(usb_gadget_dir)
    } else {
        Err(Error::new(ErrorKind::NotFound, "usb_gadget not found in configfs"))
    }
}

/// Get all USB gadgets registered on the system.
///
/// This returns all USB gadgets, including gadgets not created by the running program or
/// registered by other means than using this library.
pub fn registered() -> Result<Vec<RegGadget>> {
    let usb_gadget_dir = usb_gadget_dir()?;

    let mut gadgets = Vec::new();
    for gadget_dir in fs::read_dir(usb_gadget_dir)? {
        let Ok(gadget_dir) = gadget_dir else { continue };
        if gadget_dir.metadata()?.is_dir() {
            gadgets.push(RegGadget { dir: gadget_dir.path(), attached: false, func_dirs: HashMap::new() });
        }
    }

    Ok(gadgets)
}

/// Remove all USB gadgets defined on the system.
///
/// This removes all USB gadgets, including gadgets not created by the running program or
/// registered by other means than using this library.
pub fn remove_all() -> Result<()> {
    let mut res = Ok(());

    for gadget in registered()? {
        if let Err(err) = gadget.remove() {
            res = Err(err);
        }
    }

    res
}

/// Unbind all USB gadgets defined on the system.
///
/// This unbinds all USB gadgets, including gadgets not created by the running program or
/// registered by other means than using this library.
pub fn unbind_all() -> Result<()> {
    let mut res = Ok(());

    for gadget in registered()? {
        if let Err(err) = gadget.bind(None) {
            res = Err(err);
        }
    }

    res
}
