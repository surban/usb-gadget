//! USB device controller (UDC).

use std::{
    ffi::{OsStr, OsString},
    fmt,
    io::{Error, ErrorKind, Result},
    os::unix::prelude::OsStringExt,
    path::{Path, PathBuf},
};
use tokio::fs;

use crate::{trim_os_str, Speed};

/// USB device controller (UDC).
///
/// Call [`udcs`] to obtain the controllers available on the system.
#[derive(Clone)]
pub struct Udc {
    dir: PathBuf,
}

impl fmt::Debug for Udc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Udc").field("name", &self.name()).finish()
    }
}

impl Udc {
    /// The name of the USB device controller.
    pub fn name(&self) -> &OsStr {
        self.dir.file_name().unwrap()
    }

    /// Indicates if an OTG A-Host supports HNP at an alternate port.
    pub async fn a_alt_hnp_support(&self) -> Result<bool> {
        Ok(fs::read_to_string(self.dir.join("a_alt_hnp_support")).await?.trim() != "0")
    }

    /// Indicates if an OTG A-Host supports HNP at this port.
    pub async fn a_hnp_support(&self) -> Result<bool> {
        Ok(fs::read_to_string(self.dir.join("a_hnp_support")).await?.trim() != "0")
    }

    /// Indicates if an OTG A-Host enabled HNP support.
    pub async fn b_hnp_enable(&self) -> Result<bool> {
        Ok(fs::read_to_string(self.dir.join("b_hnp_enable")).await?.trim() != "0")
    }

    /// Indicates the current negotiated speed at this port.
    ///
    /// `None` if unknown.
    pub async fn current_speed(&self) -> Result<Speed> {
        Ok(fs::read_to_string(self.dir.join("current_speed")).await?.trim().parse().unwrap_or_default())
    }

    /// Indicates the maximum USB speed supported by this port.
    pub async fn max_speed(&self) -> Result<Speed> {
        Ok(fs::read_to_string(self.dir.join("maximum_speed")).await?.trim().parse().unwrap_or_default())
    }

    /// Indicates that this port is the default Host on an OTG session but HNP was used to switch roles.
    pub async fn is_a_peripheral(&self) -> Result<bool> {
        Ok(fs::read_to_string(self.dir.join("is_a_peripheral")).await?.trim() != "0")
    }

    /// Indicates that this port support OTG.
    pub async fn is_otg(&self) -> Result<bool> {
        Ok(fs::read_to_string(self.dir.join("is_otg")).await?.trim() != "0")
    }

    /// Indicates current state of the USB Device Controller.
    ///
    /// However not all USB Device Controllers support reporting all states.
    pub async fn state(&self) -> Result<UdcState> {
        Ok(fs::read_to_string(self.dir.join("state")).await?.trim().parse().unwrap_or_default())
    }

    /// Manually start Session Request Protocol (SRP).
    pub async fn start_srp(&self) -> Result<()> {
        fs::write(self.dir.join("srp"), "1").await
    }

    /// Connect or disconnect data pullup resistors thus causing a logical connection to or disconnection from the USB host.
    pub async fn set_soft_connect(&self, connect: bool) -> Result<()> {
        fs::write(self.dir.join("soft_connect"), if connect { "connect" } else { "disconnect" }).await
    }

    /// Name of currently running USB Gadget Driver.
    pub async fn function(&self) -> Result<Option<OsString>> {
        let data = OsString::from_vec(fs::read(self.dir.join("function")).await?);
        let data = trim_os_str(&data);
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(data.to_os_string()))
        }
    }
}

/// USB device controller (UDC) connection state.
#[derive(
    Default, Debug, strum::Display, strum::EnumString, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[non_exhaustive]
pub enum UdcState {
    /// Not attached.
    #[strum(serialize = "not attached")]
    NotAttached,
    /// Attached.
    #[strum(serialize = "attached")]
    Attached,
    /// Powered.
    #[strum(serialize = "powered")]
    Powered,
    /// Reconnecting.
    #[strum(serialize = "reconnecting")]
    Reconnecting,
    /// Unauthenticated.
    #[strum(serialize = "unauthenticated")]
    Unauthenticated,
    /// Default.
    #[strum(serialize = "default")]
    Default,
    /// Addressed.
    #[strum(serialize = "addressed")]
    Addressed,
    /// Configured.
    #[strum(serialize = "configured")]
    Configured,
    /// Suspended.
    #[strum(serialize = "suspended")]
    Suspended,
    /// Unknown state.
    #[default]
    #[strum(serialize = "UNKNOWN")]
    Unknown,
}

/// Gets the available USB device controllers (UDCs) in the system.
pub async fn udcs() -> Result<Vec<Udc>> {
    let class_dir = Path::new("/sys/class");
    if !class_dir.is_dir() {
        return Err(Error::new(ErrorKind::NotFound, "sysfs is not available"));
    }

    let udc_dir = class_dir.join("udc");
    if !udc_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut udcs = Vec::new();
    let mut entries = fs::read_dir(&udc_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        udcs.push(Udc { dir: entry.path() });
    }

    Ok(udcs)
}

/// The default USB device controller (UDC) in the system by alphabetical sorting.
///
/// A not found error is returned if no UDC is present.
pub async fn default_udc() -> Result<Udc> {
    let mut udcs = udcs().await?;
    udcs.sort_by_key(|udc| udc.name().to_os_string());
    udcs.into_iter()
        .next()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "no USB device controller (UDC) available"))
}
