//! USB gadget.

use nix::errno::Errno;
use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    fmt,
    io::{Error, ErrorKind, Result},
    os::unix::prelude::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use tokio::{fs, sync::oneshot};

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
    /// Creates a new USB device or interface class.
    pub const fn new(class: u8, sub_class: u8, protocol: u8) -> Self {
        Self { class, sub_class, protocol }
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
    /// Vendor code.
    pub vendor_code: u8,
    /// Signature.
    pub qw_sign: String,
}

impl OsDescriptor {
    /// Creates a new instance.
    pub const fn new(vendor_code: u8, qw_sign: String) -> Self {
        Self { vendor_code, qw_sign }
    }

    /// The Microsoft OS descriptor.
    pub fn microsoft() -> Self {
        Self { vendor_code: 0x02, qw_sign: "MSFT100".to_string() }
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
    pub fn new(vendor_code: u8, landing_page: impl AsRef<String>) -> Self {
        Self { version: WebUsbVersion::default(), vendor_code, landing_page: landing_page.as_ref().to_string() }
    }
}

/// USB gadget configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Maximum power in 2 mA units.
    max_power: u8,
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
    pub fn new(english_description: impl AsRef<str>) -> Self {
        Self {
            max_power: (500u16 / 2) as u8,
            self_powered: false,
            remote_wakeup: false,
            description: [(Language::EnglishUnitedStates, english_description.as_ref().to_string())].into(),
            functions: Default::default(),
        }
    }

    /// Sets the maximum power in mA.
    pub fn set_max_power_ma(&mut self, ma: u16) -> Result<()> {
        if ma > 500 {
            return Err(Error::new(ErrorKind::InvalidInput, "maximum power must not exceed 500 mA"));
        }

        self.max_power = (ma / 2) as u8;
        Ok(())
    }

    /// Adds a USB function (interface) to this configuration.
    pub fn add_function(&mut self, function_handle: function::Handle) {
        self.functions.insert(function_handle);
    }

    /// Adds a USB function (interface) to this configuration.
    pub fn with_function(mut self, function_handle: function::Handle) -> Self {
        self.add_function(function_handle);
        self
    }

    async fn register(
        &self, gadget_dir: &Path, idx: usize, func_dirs: &HashMap<function::Handle, PathBuf>,
    ) -> Result<()> {
        let dir = gadget_dir.join("configs").join(format!("c.{idx}"));
        tracing::debug!("creating config at {}", dir.display());
        fs::create_dir(&dir).await?;

        let mut attributes = 1 << 7;
        if self.self_powered {
            attributes |= 1 << 6;
        }
        if self.remote_wakeup {
            attributes |= 1 << 5;
        }

        fs::write(dir.join("bmAttributes"), hex_u8(attributes)).await?;
        fs::write(dir.join("MaxPower"), format!("{}", self.max_power)).await?;

        for (&lang, desc) in &self.description {
            let lang_dir = dir.join("strings").join(hex_u16(lang.into()));
            fs::create_dir(&lang_dir).await?;
            fs::write(lang_dir.join("configuration"), &desc).await?;
        }

        for func in &self.functions {
            let func_dir = &func_dirs[func];
            tracing::debug!("adding function {}", func_dir.display());
            fs::symlink(func_dir, dir.join(func_dir.file_name().unwrap())).await?;
        }

        Ok(())
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
    pub device_release: u16,
    /// USB specification version.
    pub usb_version: UsbVersion,
    /// Maximum speed supported by driver.
    pub max_speed: Option<Speed>,
    /// OS String extension.
    pub os_descriptor: Option<OsDescriptor>,
    /// WebUSB extension.
    pub web_usb: Option<WebUsb>,
    /// USB device configurations.
    pub configs: Vec<Config>,
}

impl Gadget {
    /// Creates a new USB gadget definition.
    pub fn new(device_class: Class, id: Id, english_strings: Strings) -> Self {
        Self {
            device_class,
            id,
            strings: [(Language::EnglishUnitedStates, english_strings)].into(),
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
    pub fn with_config(mut self, config: Config) -> Self {
        self.add_config(config);
        self
    }

    /// Bind USB gadget to a USB device controller (UDC).
    ///
    /// At least one [configuration](Config) must be added before the gadget
    /// can be bound.
    pub async fn bind(&self, udc: &Udc) -> Result<RegGadget> {
        if self.configs.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "USB gadget must have at least one configuration"));
        }

        let usb_gadget_dir = usb_gadget_dir().await?;

        let mut gadget_idx: u16 = 0;
        let dir = loop {
            let dir = usb_gadget_dir.join(format!("usb-gadget{gadget_idx}"));
            match fs::create_dir(&dir).await {
                Ok(()) => break dir,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
                Err(err) => return Err(err),
            }
            gadget_idx = gadget_idx
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorKind::OutOfMemory, "USB gadgets exhausted"))?;
        };

        tracing::debug!("creating gadget at {}", dir.display());

        fs::write(dir.join("bDeviceClass"), hex_u8(self.device_class.class)).await?;
        fs::write(dir.join("bDeviceSubClass"), hex_u8(self.device_class.sub_class)).await?;
        fs::write(dir.join("bDeviceProtocol"), hex_u8(self.device_class.protocol)).await?;

        fs::write(dir.join("idVendor"), hex_u16(self.id.vendor)).await?;
        fs::write(dir.join("idProduct"), hex_u16(self.id.product)).await?;

        fs::write(dir.join("bMaxPacketSize0"), hex_u8(self.max_packet_size0)).await?;
        fs::write(dir.join("bcdDevice"), hex_u16(self.device_release)).await?;
        fs::write(dir.join("bcdUSB"), hex_u16(self.usb_version.into())).await?;

        if let Some(v) = self.max_speed {
            fs::write(dir.join("max_speed"), v.to_string()).await?;
        }

        if let Some(os_desc) = &self.os_descriptor {
            let os_desc_dir = dir.join("os_desc");
            if os_desc_dir.is_dir() {
                fs::write(os_desc_dir.join("b_vendor_code"), hex_u8(os_desc.vendor_code)).await?;
                fs::write(os_desc_dir.join("qw_sign"), &os_desc.qw_sign).await?;
                fs::write(os_desc_dir.join("use"), "1").await?;
            } else {
                tracing::warn!("USB OS descriptor is unsupported by kernel");
            }
        }

        if let Some(webusb) = &self.web_usb {
            let webusb_dir = dir.join("webusb");
            if webusb_dir.is_dir() {
                fs::write(webusb_dir.join("bVendorCode"), hex_u8(webusb.vendor_code)).await?;
                fs::write(webusb_dir.join("bcdVersion"), hex_u16(webusb.version.into())).await?;
                fs::write(webusb_dir.join("landingPage"), &webusb.landing_page).await?;
                fs::write(webusb_dir.join("use"), "1").await?;
            } else {
                tracing::warn!("WebUSB descriptor is unsupported by kernel");
            }
        }

        for (&lang, strs) in &self.strings {
            let lang_dir = dir.join("strings").join(hex_u16(lang.into()));
            fs::create_dir(&lang_dir).await?;

            fs::write(lang_dir.join("manufacturer"), &strs.manufacturer).await?;
            fs::write(lang_dir.join("product"), &strs.product).await?;
            fs::write(lang_dir.join("serialnumber"), &strs.serial_number).await?;
        }

        let functions: HashSet<_> = self.configs.iter().flat_map(|c| &c.functions).collect();
        let mut func_dirs = HashMap::new();
        for (func_idx, &func) in functions.iter().enumerate() {
            let func_dir = dir.join(
                dir.join("functions")
                    .join(format!("{}.usb-gadget{gadget_idx}-{func_idx}", func.get().driver().to_str().unwrap())),
            );
            tracing::debug!("creating function at {}", func_dir.display());
            fs::create_dir(&func_dir).await?;

            func.get().dir().set_dir(&func_dir);
            func.get().register().await?;

            func_dirs.insert(func.clone(), func_dir);
        }

        for (idx, config) in self.configs.iter().enumerate() {
            config.register(&dir, idx + 1, &func_dirs).await?;
        }

        tracing::debug!("binding gadget to UDC {}", udc.name().to_string_lossy());
        fs::write(dir.join("UDC"), udc.name().as_bytes()).await?;

        let (keep_tx, keep_rx) = oneshot::channel();
        let remove_funcs = func_dirs.clone();
        let remove_dir = dir.clone();
        tokio::spawn(async move {
            match keep_rx.await {
                Ok(()) => (),
                Err(_) => {
                    tracing::debug!("removing gadget at {}", remove_dir.display());

                    for func in remove_funcs.keys() {
                        let _ = func.get().pre_removal().await;
                    }

                    let _ = remove_at(&remove_dir).await;

                    for (func, dir) in &remove_funcs {
                        func.get().dir().reset_dir();
                        let _ = func.get().post_removal(dir).await;
                    }
                }
            }
        });

        tracing::debug!("gadget at {} created", dir.display());
        Ok(RegGadget { dir, keep_tx: Some(keep_tx), func_dirs })
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
    keep_tx: Option<oneshot::Sender<()>>,
    func_dirs: HashMap<Handle, PathBuf>,
}

impl fmt::Debug for RegGadget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RegisteredGadget")
            .field("name", &self.name())
            .field("is_attached", &self.is_attached())
            .finish()
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
        self.keep_tx.is_some()
    }

    /// The name of the USB device controller (UDC) this gadget is bound to.
    pub async fn udc(&self) -> Result<Option<OsString>> {
        let data = OsString::from_vec(fs::read(self.dir.join("UDC")).await?);
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
    pub async fn bind(&self, udc: Option<&Udc>) -> Result<()> {
        tracing::debug!("binding gadget {:?} to {:?}", self, &udc);

        let name = match udc {
            Some(udc) => udc.name().to_os_string(),
            None => "\n".into(),
        };

        match fs::write(self.dir.join("UDC"), name.as_bytes()).await {
            Ok(()) => Ok(()),
            Err(err) if udc.is_none() && err.raw_os_error() == Some(Errno::ENODEV as i32) => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Detach the handle from the USB gadget while keeping the USB gadget active.
    pub fn detach(&mut self) {
        if let Some(keep_tx) = self.keep_tx.take() {
            let _ = keep_tx.send(());
        }
    }

    /// Unbind from the UDC and remove the USB gadget.
    pub async fn remove(mut self) -> Result<()> {
        for func in self.func_dirs.keys() {
            func.get().pre_removal().await?;
        }

        remove_at(&self.dir).await?;

        for func in self.func_dirs.keys() {
            func.get().dir().reset_dir();
        }

        for (func, dir) in &self.func_dirs {
            func.get().post_removal(dir).await?;
        }

        self.detach();
        Ok(())
    }
}

impl Drop for RegGadget {
    fn drop(&mut self) {
        // empty
    }
}

/// Remove USB gadget at specified configfs gadget directory.
async fn remove_at(dir: &Path) -> Result<()> {
    tracing::debug!("removing gadget at {}", dir.display());

    init_remove_handlers();

    let _ = fs::write(dir.join("UDC"), "\n").await;

    let mut configs = fs::read_dir(dir.join("configs")).await?;
    while let Some(config_dir) = configs.next_entry().await? {
        if !config_dir.metadata().await?.is_dir() {
            continue;
        }

        let mut funcs = fs::read_dir(config_dir.path()).await?;
        while let Some(func) = funcs.next_entry().await? {
            if func.metadata().await?.is_symlink() {
                fs::remove_file(func.path()).await?;
            }
        }

        let mut langs = fs::read_dir(config_dir.path().join("strings")).await?;
        while let Some(lang) = langs.next_entry().await? {
            if lang.metadata().await?.is_dir() {
                fs::remove_dir(lang.path()).await?;
            }
        }

        fs::remove_dir(config_dir.path()).await?;
    }

    let mut functions = fs::read_dir(dir.join("functions")).await?;
    while let Some(func_dir) = functions.next_entry().await? {
        if !func_dir.metadata().await?.is_dir() {
            continue;
        }

        call_remove_handler(&func_dir.path()).await?;

        fs::remove_dir(func_dir.path()).await?;
    }

    let mut langs = fs::read_dir(dir.join("strings")).await?;
    while let Some(lang) = langs.next_entry().await? {
        if lang.metadata().await?.is_dir() {
            fs::remove_dir(lang.path()).await?;
        }
    }

    fs::remove_dir(dir).await?;

    tracing::debug!("removed gadget at {}", dir.display());
    Ok(())
}

/// The path to the USB gadget configuration directory within configfs.
async fn usb_gadget_dir() -> Result<PathBuf> {
    let _ = request_module("libcomposite").await;

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
pub async fn registered() -> Result<Vec<RegGadget>> {
    let usb_gadget_dir = usb_gadget_dir().await?;

    let mut gadgets = Vec::new();
    let mut gadget_dirs = fs::read_dir(usb_gadget_dir).await?;
    while let Some(gadget_dir) = gadget_dirs.next_entry().await? {
        if gadget_dir.metadata().await?.is_dir() {
            gadgets.push(RegGadget { dir: gadget_dir.path(), keep_tx: None, func_dirs: HashMap::new() });
        }
    }

    Ok(gadgets)
}

/// Remove all USB gadgets defined on the system.
///
/// This removes all USB gadgets, including gadgets not created by the running program or
/// registered by other means than using this library.
pub async fn remove_all() -> Result<()> {
    let mut res = Ok(());

    for gadget in registered().await? {
        if let Err(err) = gadget.remove().await {
            res = Err(err);
        }
    }

    res
}

/// Unbind all USB gadgets defined on the system.
///
/// This unbinds all USB gadgets, including gadgets not created by the running program or
/// registered by other means than using this library.
pub async fn unbind_all() -> Result<()> {
    let mut res = Ok(());

    for gadget in registered().await? {
        if let Err(err) = gadget.bind(None).await {
            res = Err(err);
        }
    }

    res
}
