//! Utils for implementing USB gadget functions.

use async_trait::async_trait;
use futures_util::FutureExt;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt,
    future::Future,
    io::{Error, ErrorKind, Result},
    os::unix::prelude::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, Once, OnceLock},
};
use tokio::fs;

use crate::{function::register_remove_handlers, trim_os_str};

/// USB gadget function.
#[async_trait]
pub trait Function: fmt::Debug + Send + Sync + 'static {
    /// Name of the function driver.
    fn driver(&self) -> OsString;

    /// Function directory.
    fn dir(&self) -> FunctionDir;

    /// Register the function in configfs at the specified path.
    async fn register(&self) -> Result<()>;

    /// Notifies the function that the USB gadget is about to be removed.
    async fn pre_removal(&self) -> Result<()> {
        Ok(())
    }

    /// Notifies the function that the USB gadget has been removed.
    async fn post_removal(&self, _dir: &Path) -> Result<()> {
        Ok(())
    }
}

/// USB gadget function directory container.
///
/// Stores the directory in configfs of a USB funciton and provides access methods.
#[derive(Clone)]
pub struct FunctionDir(Arc<Mutex<Option<PathBuf>>>);

impl fmt::Debug for FunctionDir {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("FunctionDir").field(&*self.0.lock().unwrap()).finish()
    }
}

impl Default for FunctionDir {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionDir {
    /// Creates an empty function directory container.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    pub(crate) fn set_dir(&self, function_dir: &Path) {
        *self.0.lock().unwrap() = Some(function_dir.to_path_buf());
    }

    pub(crate) fn reset_dir(&self) {
        *self.0.lock().unwrap() = None;
    }

    /// The USB gadget function directory in configfs.
    pub fn dir(&self) -> Result<PathBuf> {
        self.0
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "USB function not registered"))
    }

    /// Driver name.
    pub fn driver(&self) -> Result<OsString> {
        let dir = self.dir()?;
        let (driver, _instance) = split_function_dir(&dir).unwrap();
        Ok(driver.to_os_string())
    }

    /// Instance name.
    pub fn instance(&self) -> Result<OsString> {
        let dir = self.dir()?;
        let (_driver, instance) = split_function_dir(&dir).unwrap();
        Ok(instance.to_os_string())
    }

    /// Path to the specified property.
    pub fn property_path(&self, name: impl AsRef<Path>) -> Result<PathBuf> {
        let path = name.as_ref();
        if path.components().all(|c| matches!(c, Component::Normal(_))) {
            Ok(self.dir()?.join(path))
        } else {
            Err(Error::new(ErrorKind::InvalidInput, "property path must be relative"))
        }
    }

    /// Create a subdirectory.
    pub async fn create_dir(&self, name: impl AsRef<Path>) -> Result<()> {
        let path = self.property_path(name)?;
        tracing::debug!("creating directory {}", path.display());
        fs::create_dir(path).await
    }

    /// Remove a subdirectory.
    pub async fn remove_dir(&self, name: impl AsRef<Path>) -> Result<()> {
        let path = self.property_path(name)?;
        tracing::debug!("removing directory {}", path.display());
        fs::remove_dir(path).await
    }

    /// Read a binary property.
    pub async fn read(&self, name: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = self.property_path(name)?;
        let res = fs::read(&path).await;

        match &res {
            Ok(value) => {
                tracing::debug!("read property {} with value {}", path.display(), String::from_utf8_lossy(value))
            }
            Err(err) => tracing::debug!("reading property {} failed: {}", path.display(), err),
        }

        res
    }

    /// Read and trim a string property.
    pub async fn read_string(&self, name: impl AsRef<Path>) -> Result<String> {
        let mut data = self.read(name).await?;
        while data.last() == Some(&b'\0') || data.last() == Some(&b' ') || data.last() == Some(&b'\n') {
            data.truncate(data.len() - 1);
        }

        Ok(String::from_utf8(data).map_err(|err| Error::new(ErrorKind::InvalidData, err))?.trim().to_string())
    }

    /// Read an trim an OS string property.
    pub async fn read_os_string(&self, name: impl AsRef<Path>) -> Result<OsString> {
        Ok(trim_os_str(&OsString::from_vec(self.read(name).await?)).to_os_string())
    }

    /// Write a property.
    pub async fn write(&self, name: impl AsRef<Path>, value: impl AsRef<[u8]>) -> Result<()> {
        let path = self.property_path(name)?;
        let value = value.as_ref();
        tracing::debug!("setting property {} to {}", path.display(), String::from_utf8_lossy(value));
        fs::write(path, value).await
    }
}

/// Split configfs function directory path into driver name and instance name.
pub fn split_function_dir(function_dir: &Path) -> Option<(&OsStr, &OsStr)> {
    let Some(name) = function_dir.file_name() else { return None };
    let name = name.as_bytes();

    let Some(dot) = name.iter().enumerate().find_map(|(i, c)| if *c == b'.' { Some(i) } else { None }) else {
        return None;
    };
    let driver = &name[..dot];
    let instance = &name[dot + 1..];

    Some((OsStr::from_bytes(driver), OsStr::from_bytes(instance)))
}

/// Handler function for removing function instance.
type RemoveHandler = Arc<dyn Fn(PathBuf) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

/// Registered handlers for removing function instances.
static REMOVE_HANDLERS: OnceLock<Mutex<HashMap<OsString, RemoveHandler>>> = OnceLock::new();

/// Registered handlers for removing function instances.
fn remove_handlers() -> MutexGuard<'static, HashMap<OsString, RemoveHandler>> {
    let handlers = REMOVE_HANDLERS.get_or_init(|| Mutex::new(HashMap::new()));
    handlers.lock().unwrap()
}

/// Initializes handlers for removing function instances.
pub(crate) fn init_remove_handlers() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        register_remove_handlers();
    });
}

/// Register a function remove handler for the specified function driver.
pub fn register_remove_handler<Fut>(
    driver: impl AsRef<OsStr>, handler: impl Fn(PathBuf) -> Fut + Send + Sync + 'static,
) where
    Fut: Future<Output = Result<()>> + Send + Sync + 'static,
{
    remove_handlers().insert(driver.as_ref().to_os_string(), Arc::new(move |dir| handler(dir).boxed()));
}

/// Calls the remove handler for the function directory, if any is registered.
pub(crate) async fn call_remove_handler(function_dir: &Path) -> Result<()> {
    let Some((driver, _)) = split_function_dir(function_dir) else {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid function directory"));
    };

    let handler_opt = remove_handlers().get(driver).cloned();
    match handler_opt {
        Some(handler) => handler(function_dir.to_path_buf()).await,
        None => Ok(()),
    }
}
