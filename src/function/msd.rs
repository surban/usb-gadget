//! Mass Storage Device (MSD) function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_MASS_STORAGE` must be enabled.

use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{Error, ErrorKind, Result},
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
};

use super::{util::FunctionDir, Function, Handle};

pub(crate) fn driver() -> &'static OsStr {
    OsStr::new("mass_storage")
}

/// Logical unit (LUN) of mass storage device (MSD).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Lun {
    /// Flag specifying access to the LUN shall be read-only.
    ///
    /// This is implied if CD-ROM emulation is enabled as well as
    /// when it was impossible to open the backing file in R/W mode.
    pub read_only: bool,
    /// Flag specifying that LUN shall be reported as being a CD-ROM.
    pub cdrom: bool,
    /// Flag specifying that FUA flag in SCSI WRITE(10,12).
    pub no_fua: bool,
    /// Flag specifying that LUN shall be indicated as being removable.
    pub removable: bool,
    /// The path to the backing file for the LUN.
    ///
    /// Required if LUN is not marked as removable.
    file: Option<PathBuf>,
    /// Inquiry string.
    pub inquiry_string: String,
}

impl Lun {
    /// Create a new LUN backed by the specified file.
    pub fn new(file: impl AsRef<Path>) -> Result<Self> {
        let mut this = Self::default();
        this.set_file(Some(file))?;
        Ok(this)
    }

    /// Creates a new LUN without a medium.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Set the path to the backing file for the LUN.
    pub fn set_file<F: AsRef<Path>>(&mut self, file: Option<F>) -> Result<()> {
        match file {
            Some(file) => {
                let file = file.as_ref();
                if !file.is_absolute() {
                    return Err(Error::new(ErrorKind::InvalidInput, "the LUN file path must be absolute"));
                }
                self.file = Some(file.to_path_buf());
            }
            None => self.file = None,
        }

        Ok(())
    }

    fn dir_name(idx: usize) -> String {
        format!("lun.{idx}")
    }
}

impl Default for Lun {
    fn default() -> Self {
        Self {
            read_only: false,
            cdrom: false,
            no_fua: false,
            removable: true,
            file: None,
            inquiry_string: String::new(),
        }
    }
}

/// Builder for USB Mass Storage Device (MSD) function.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MsdBuilder {
    /// Set to permit function to halt bulk endpoints.
    ///
    /// Disabled on some USB devices known not to work correctly.
    pub stall: Option<bool>,
    /// Logical units.
    pub luns: Vec<Lun>,
}

impl MsdBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Msd, Handle) {
        let dir = FunctionDir::new();
        (Msd { dir: dir.clone() }, Handle::new(MsdFunction { builder: self, dir }))
    }

    /// Adds a LUN.
    pub fn add_lun(&mut self, lun: Lun) {
        self.luns.push(lun);
    }

    /// Adds a LUN.
    pub fn with_lun(mut self, lun: Lun) -> Self {
        self.add_lun(lun);
        self
    }
}

#[derive(Debug)]
struct MsdFunction {
    builder: MsdBuilder,
    dir: FunctionDir,
}

impl Function for MsdFunction {
    fn driver(&self) -> OsString {
        driver().into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if self.builder.luns.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "at least one LUN must exist"));
        }

        if let Some(stall) = self.builder.stall {
            self.dir.write("stall", if stall { "1" } else { "0" })?;
        }

        for (idx, lun) in self.builder.luns.iter().enumerate() {
            let lun_dir_name = Lun::dir_name(idx);

            if idx != 0 {
                self.dir.create_dir(&lun_dir_name)?;
            }

            self.dir.write(format!("{lun_dir_name}/ro"), if lun.read_only { "1" } else { "0" })?;
            self.dir.write(format!("{lun_dir_name}/cdrom"), if lun.cdrom { "1" } else { "0" })?;
            self.dir.write(format!("{lun_dir_name}/nofua"), if lun.no_fua { "1" } else { "0" })?;
            self.dir.write(format!("{lun_dir_name}/removable"), if lun.removable { "1" } else { "0" })?;
            self.dir.write(format!("{lun_dir_name}/inquiry_string"), &lun.inquiry_string)?;
            if let Some(file) = &lun.file {
                self.dir.write(format!("{lun_dir_name}/file"), file.as_os_str().as_bytes())?;
            }
        }

        Ok(())
    }
}

/// USB Mass Storage Device (MSD) function.
#[derive(Debug)]
pub struct Msd {
    dir: FunctionDir,
}

impl Msd {
    /// Creates a new USB Mass Storage Device (MSD) with the specified backing file.
    pub fn new(file: impl AsRef<Path>) -> Result<(Msd, Handle)> {
        let mut builder = Self::builder();
        builder.luns.push(Lun::new(file)?);
        Ok(builder.build())
    }

    /// Creates a new USB Mass Storage Device (MSD) builder.
    pub fn builder() -> MsdBuilder {
        MsdBuilder { stall: None, luns: Vec::new() }
    }

    /// Path of this USB function in configfs.
    pub fn path(&self) -> Result<PathBuf> {
        self.dir.dir()
    }

    /// Forcibly detach the backing file from the LUN, regardless of whether the host has allowed it.
    pub fn force_eject(&self, lun: usize) -> Result<()> {
        let lun_dir_name = Lun::dir_name(lun);
        self.dir.write(format!("{lun_dir_name}/forced_eject"), "1")
    }

    /// Set the path to the backing file for the LUN.
    pub fn set_file<P: AsRef<Path>>(&self, lun: usize, file: Option<P>) -> Result<()> {
        let lun_dir_name = Lun::dir_name(lun);
        let file = match file {
            Some(file) => {
                let file = file.as_ref();
                if !file.is_absolute() {
                    return Err(Error::new(ErrorKind::InvalidInput, "the LUN file path must be absolute"));
                }
                file.as_os_str().as_bytes().to_vec()
            }
            None => Vec::new(),
        };
        self.dir.write(format!("{lun_dir_name}/file"), file)
    }
}

pub(crate) fn remove_handler(dir: PathBuf) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        if entry.file_type()?.is_dir()
            && entry.file_name().as_bytes().contains(&b'.')
            && entry.file_name() != "lun.0"
        {
            fs::remove_dir(entry.path())?;
        }
    }

    Ok(())
}
