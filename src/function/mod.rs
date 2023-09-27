//! USB gadget functions.

use async_trait::async_trait;
use futures_util::FutureExt;
use std::{
    cmp,
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt,
    future::Future,
    hash,
    hash::Hash,
    io::{Error, ErrorKind, Result},
    ops::Deref,
    os::unix::prelude::OsStrExt,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

mod custom;
pub use custom::*;

mod other;
pub use other::*;

#[async_trait]
pub trait Function: fmt::Debug + Send + Sync + 'static {
    /// Name of the function driver.
    fn driver(&self) -> OsString;

    /// Register the function in configfs at the specified path.
    async fn register(&self, dir: &Path) -> Result<()>;

    //async fn cleanup
}

/// A function handle.
#[derive(Debug, Clone)]
pub struct Handle(Arc<dyn Function>);

impl Handle {
    pub fn new<F: Function>(f: F) -> Self {
        Self(Arc::new(f))
    }
}

impl Deref for Handle {
    type Target = dyn Function;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Handle {}

impl PartialOrd for Handle {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Arc::as_ptr(&self.0).partial_cmp(&Arc::as_ptr(&other.0))
    }
}

impl Ord for Handle {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        Arc::as_ptr(&self.0).cmp(&Arc::as_ptr(&other.0))
    }
}

impl Hash for Handle {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

type RemoveHandler = Arc<dyn Fn(&Path) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

static REMOVE_HANDLERS: OnceLock<Mutex<HashMap<OsString, RemoveHandler>>> = OnceLock::new();

fn remove_handlers() -> MutexGuard<'static, HashMap<OsString, RemoveHandler>> {
    let handlers = REMOVE_HANDLERS.get_or_init(|| Mutex::new(HashMap::new()));
    handlers.lock().unwrap()
}

/// Register a function remove handler for the specified function driver.
pub fn register_remove_handler<Fut>(
    driver: impl AsRef<OsStr>, handler: impl Fn(&Path) -> Fut + Send + Sync + 'static,
) where
    Fut: Future<Output = Result<()>> + Send + Sync + 'static,
{
    remove_handlers().insert(driver.as_ref().to_os_string(), Arc::new(move |dir| handler(dir).boxed()));
}

/// Calls the remove handler for the function directory, if any is registered.
pub(crate) async fn call_remove_handler(function_dir: &Path) -> Result<()> {
    let Some((driver, _)) = split_function_dir(&function_dir) else {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid function directory"));
    };

    let handler_opt = remove_handlers().get(driver).cloned();
    match handler_opt {
        Some(handler) => handler(function_dir).await,
        None => Ok(()),
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
