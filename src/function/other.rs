use async_trait::async_trait;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    os::unix::prelude::OsStrExt,
    path::{Component, Path, PathBuf},
};
use tokio::fs;

use super::{Function, Handle};

/// Other USB function implemented by a kernel function driver.
#[derive(Debug, Clone)]
pub struct OtherBuilder {
    /// Function driver name.
    driver: OsString,
    /// Properties to set.
    properties: HashMap<PathBuf, Vec<u8>>,
}

impl OtherBuilder {
    /// Create a new other function implemented by the specified kernel function driver.
    ///
    /// Driver name `xxx` corresponds to kernel module `usb_f_xxx.ko`.
    pub fn new(driver: impl AsRef<OsStr>) -> Result<Self> {
        let driver = driver.as_ref();
        if driver.as_bytes().contains(&b'.') || driver.as_bytes().contains(&b'/') || !driver.is_ascii() {
            return Err(Error::new(ErrorKind::InvalidInput, "invalid driver name"));
        }

        Ok(Self { driver: driver.to_os_string(), properties: HashMap::new() })
    }

    /// Set a property value.
    pub fn set(&mut self, name: impl AsRef<Path>, value: impl AsRef<[u8]>) -> Result<()> {
        let path = name.as_ref().to_path_buf();
        if !path.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(Error::new(ErrorKind::InvalidInput, "property path must only contain normal components"));
        }

        self.properties.insert(path, value.as_ref().to_vec());
        Ok(())
    }

    pub fn build(self) -> Handle {
        Handle::new(self)
    }
}

#[async_trait]
impl Function for OtherBuilder {
    fn driver(&self) -> OsString {
        self.driver.clone()
    }

    async fn register(&self, dir: &Path) -> Result<()> {
        for (prop, val) in &self.properties {
            fs::write(dir.join(prop), val).await?;
        }

        Ok(())
    }
}
