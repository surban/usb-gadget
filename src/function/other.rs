//! Other USB function.

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    os::unix::prelude::OsStrExt,
    path::{Component, Path, PathBuf},
};

use super::{util::FunctionDir, Function, Handle};

/// Builder for other USB function implemented by a kernel function driver.
#[derive(Debug, Clone)]
pub struct OtherBuilder {
    /// Function driver name.
    driver: OsString,
    /// Properties to set.
    properties: HashMap<PathBuf, Vec<u8>>,
}

impl OtherBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Other, Handle) {
        let dir = FunctionDir::new();
        (Other { dir: dir.clone() }, Handle::new(OtherFunction { builder: self, dir }))
    }

    /// Set a property value.
    pub fn set(&mut self, name: impl AsRef<Path>, value: impl AsRef<[u8]>) -> Result<()> {
        let path = name.as_ref().to_path_buf();
        if !path.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(Error::new(ErrorKind::InvalidInput, "property path must be relative"));
        }

        self.properties.insert(path, value.as_ref().to_vec());
        Ok(())
    }
}

#[derive(Debug)]
struct OtherFunction {
    builder: OtherBuilder,
    dir: FunctionDir,
}

impl Function for OtherFunction {
    fn driver(&self) -> OsString {
        self.builder.driver.clone()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        for (prop, val) in &self.builder.properties {
            self.dir.write(prop, val)?;
        }

        Ok(())
    }
}

/// Other USB function implemented by a kernel function driver.
///
/// Driver name `xxx` corresponds to kernel module `usb_f_xxx.ko`.
#[derive(Debug)]
pub struct Other {
    dir: FunctionDir,
}

impl Other {
    /// Create a new other function implemented by the specified kernel function driver.
    pub fn new(driver: impl AsRef<OsStr>) -> Result<(Other, Handle)> {
        Ok(Self::builder(driver)?.build())
    }

    /// Build a new other function implemented by the specified kernel function driver.
    pub fn builder(driver: impl AsRef<OsStr>) -> Result<OtherBuilder> {
        let driver = driver.as_ref();
        if driver.as_bytes().contains(&b'.') || driver.as_bytes().contains(&b'/') || !driver.is_ascii() {
            return Err(Error::new(ErrorKind::InvalidInput, "invalid driver name"));
        }

        Ok(OtherBuilder { driver: driver.to_os_string(), properties: HashMap::new() })
    }

    /// Path of this USB function in configfs.
    pub fn path(&self) -> Result<PathBuf> {
        self.dir.dir()
    }

    /// Get a property value.
    pub fn get(&self, name: impl AsRef<Path>) -> Result<Vec<u8>> {
        self.dir.read(name)
    }
}
