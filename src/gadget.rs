//! USB gadget.

use std::fmt;
use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    os::unix::prelude::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{self, symlink},
    sync::oneshot,
};

use crate::trim_os_str;
use crate::{configfs_dir, function, hex_u16, hex_u8, lang::Language, udc::Udc, Speed};

/// USB device or interface class.
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

/// USB device id.
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

/// USB device strings.
#[derive(Debug, Clone)]
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
    pub fn new(manufacturer: &str, product: &str, serial_number: &str) -> Self {
        Self {
            manufacturer: manufacturer.to_string(),
            product: product.to_string(),
            serial_number: serial_number.to_string(),
        }
    }
}

/// USB gadget operating system descriptor.
#[derive(Debug, Clone)]
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
    pub fn new(english_description: &str) -> Self {
        Self {
            max_power: (500u16 / 2) as u8,
            self_powered: false,
            remote_wakeup: false,
            description: [(Language::EnglishUnitedStates, english_description.to_string())].into(),
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

    async fn register(
        &self, gadget_dir: &Path, idx: usize, func_dirs: &HashMap<function::Handle, PathBuf>,
    ) -> Result<()> {
        let dir = gadget_dir.join("configs").join(format!("c.{idx}"));
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
            let func_dir = &func_dirs[&func];
            symlink(func_dir, dir.join(func_dir.file_name().unwrap())).await?;
        }

        Ok(())
    }
}

/// An USB gadget definition.
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
    pub b_max_packet_size0: Option<u8>,
    /// Device release number in BCD format.
    pub bcd_device: Option<u16>,
    /// USB specification version number in BCD format.
    pub bcd_usb: Option<u16>,
    /// Maximum speed supported by driver.
    pub max_speed: Option<Speed>,
    /// Operating system descriptor.
    pub os_descriptor: Option<OsDescriptor>,
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
            b_max_packet_size0: None,
            bcd_device: None,
            bcd_usb: None,
            max_speed: None,
            os_descriptor: None,
            configs: Vec::new(),
        }
    }

    /// Adds a USB device configuration.
    pub fn add_config(&mut self, config: Config) {
        self.configs.push(config);
    }

    /// Bind USB gadget to a USB device controller (UDC).
    pub async fn bind(&self, udc: &Udc) -> Result<RegisteredGadget> {
        let usb_gadget_dir = usb_gadget_dir().await?;

        let mut gadget_idx = 0;
        let dir = loop {
            let dir = usb_gadget_dir.join(format!("usb-gadget{gadget_idx}"));
            match fs::create_dir(&dir).await {
                Ok(()) => break dir,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
                Err(err) => return Err(err),
            }
            gadget_idx += 1;
        };

        fs::write(dir.join("bDeviceClass"), hex_u8(self.device_class.class)).await?;
        fs::write(dir.join("bDeviceSubClass"), hex_u8(self.device_class.sub_class)).await?;
        fs::write(dir.join("bDeviceProtocol"), hex_u8(self.device_class.protocol)).await?;

        fs::write(dir.join("idVendor"), hex_u16(self.id.vendor)).await?;
        fs::write(dir.join("idProduct"), hex_u16(self.id.product)).await?;

        if let Some(v) = self.b_max_packet_size0 {
            fs::write(dir.join("bMaxPacketSize0"), hex_u8(v)).await?;
        }

        if let Some(v) = self.bcd_device {
            fs::write(dir.join("bcdDevice"), hex_u16(v)).await?;
        }

        if let Some(v) = self.bcd_usb {
            fs::write(dir.join("bcdUSB"), hex_u16(v)).await?;
        }

        if let Some(v) = self.max_speed {
            fs::write(dir.join("max_speed"), v.to_string()).await?;
        }

        if let Some(os_desc) = &self.os_descriptor {
            let os_desc_dir = dir.join("os_desc");
            fs::write(os_desc_dir.join("b_vendor_code"), hex_u8(os_desc.vendor_code)).await?;
            fs::write(os_desc_dir.join("qw_sign"), &os_desc.qw_sign).await?;
            fs::write(os_desc_dir.join("use"), "1").await?;
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
                    .join(format!("{}.usb-gadget{gadget_idx}-{func_idx}", func.driver().to_str().unwrap())),
            );
            fs::create_dir(&func_dir).await?;
            func.register(&func_dir).await?;
            func_dirs.insert(func.clone(), func_dir);
        }

        for (idx, config) in self.configs.iter().enumerate() {
            config.register(&dir, idx, &func_dirs).await?;
        }

        fs::write(dir.join("UDC"), udc.name().as_bytes()).await?;

        let (keep_tx, keep_rx) = oneshot::channel();
        let remove_dir = dir.clone();
        tokio::spawn(async move {
            match keep_rx.await {
                Ok(()) => (),
                Err(_) => {
                    let _ = remove_at(&remove_dir).await;
                }
            }
        });

        Ok(RegisteredGadget { dir, keep_tx: Some(keep_tx) })
    }
}

/// An USB gadget registered with the system.
///
/// If this was obtained by calling [`Gadget::bind`], the USB gadget will be unbound and removed when this is dropped.
pub struct RegisteredGadget {
    dir: PathBuf,
    keep_tx: Option<oneshot::Sender<()>>,
}

impl fmt::Debug for RegisteredGadget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RegisteredGadget")
            .field("name", &self.name())
            .field("is_attached", &self.is_attached())
            .finish()
    }
}

impl RegisteredGadget {
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

    /// Returns the name of the USB device controller (UDC) this gadget is bound to.
    pub async fn udc(&self) -> Result<Option<OsString>> {
        let data = OsString::from_vec(fs::read(self.dir.join("UDC")).await?);
        let data = trim_os_str(&data);
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(data.to_os_string()))
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
        self.detach();
        remove_at(&self.dir).await
    }
}

impl Drop for RegisteredGadget {
    fn drop(&mut self) {
        // empty
    }
}

/// Remove USB gadget at specified configfs gadget directory.
async fn remove_at(dir: &Path) -> Result<()> {
    let _ = fs::write(dir.join("UDC"), "").await;

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

        function::call_remove_handler(&func_dir.path()).await?;

        fs::remove_dir(func_dir.path()).await?;
    }

    let mut langs = fs::read_dir(dir.join("strings")).await?;
    while let Some(lang) = langs.next_entry().await? {
        if lang.metadata().await?.is_dir() {
            fs::remove_dir(lang.path()).await?;
        }
    }

    fs::remove_dir(dir).await?;

    Ok(())
}

/// The path to the USB gadget configuration directory within configfs.
async fn usb_gadget_dir() -> Result<PathBuf> {
    let config_fs = configfs_dir()?;
    let usb_gadget_dir = config_fs.join("usb_gadget");
    if usb_gadget_dir.is_dir() {
        Ok(usb_gadget_dir)
    } else {
        Err(Error::new(ErrorKind::NotFound, "usb_gadget not found in configfs"))
    }
}

/// Gets the list of all USB gadgets defined on the system.
///
/// This returns all USB gadgets, including gadgets not created by the running program or by
/// other means than using this library.
pub async fn enumerate() -> Result<Vec<RegisteredGadget>> {
    let usb_gadget_dir = usb_gadget_dir().await?;

    let mut gadgets = Vec::new();
    let mut gadget_dirs = fs::read_dir(usb_gadget_dir).await?;
    while let Some(gadget_dir) = gadget_dirs.next_entry().await? {
        if gadget_dir.metadata().await?.is_dir() {
            gadgets.push(RegisteredGadget { dir: gadget_dir.path(), keep_tx: None });
        }
    }

    Ok(gadgets)
}

/// Remove all USB gadgets defined on the system.
///
/// This removes all USB gadgets, including gadgets not created by the running program or by
/// other means than using this library.
pub async fn remove_all() -> Result<()> {
    let mut res = Ok(());

    for gadget in enumerate().await? {
        if let Err(err) = gadget.remove().await {
            res = Err(err);
        }
    }

    res
}
