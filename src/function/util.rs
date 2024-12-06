//! Utils for implementing USB gadget functions.

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{Error, ErrorKind, Result},
    os::unix::prelude::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, Once, OnceLock},
};

use crate::{function::register_remove_handlers, trim_os_str};

/// USB gadget function.
pub trait Function: fmt::Debug + Send + Sync + 'static {
    /// Name of the function driver.
    fn driver(&self) -> OsString;

    /// Function directory.
    fn dir(&self) -> FunctionDir;

    /// Register the function in configfs at the specified path.
    fn register(&self) -> Result<()>;

    /// Notifies the function that the USB gadget is about to be removed.
    fn pre_removal(&self) -> Result<()> {
        Ok(())
    }

    /// Notifies the function that the USB gadget has been removed.
    fn post_removal(&self, _dir: &Path) -> Result<()> {
        Ok(())
    }
}

/// USB function registration state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// Function is not registered.
    Unregistered,
    /// Function is registered but not bound to UDC.
    Registered,
    /// Function is registered and bound to UDC.
    Bound,
    /// Function was removed and will stay in this state.
    Removed,
}

/// Provides access to the status of a USB function.
#[derive(Clone, Debug)]
pub struct Status(FunctionDir);

impl Status {
    /// Registration state.
    pub fn state(&self) -> State {
        let inner = self.0.inner.lock().unwrap();
        match (&inner.dir, inner.dir_was_set, inner.bound) {
            (None, false, _) => State::Unregistered,
            (None, true, _) => State::Removed,
            (Some(_), _, false) => State::Registered,
            (Some(_), _, true) => State::Bound,
        }
    }

    /// Waits for the function to be bound to a UDC.
    ///
    /// Returns with a broken pipe error if gadget is removed.
    #[cfg(feature = "tokio")]
    pub async fn bound(&self) -> Result<()> {
        loop {
            let notifier = self.0.notify.notified();
            match self.state() {
                State::Bound => return Ok(()),
                State::Removed => return Err(Error::new(ErrorKind::BrokenPipe, "gadget was removed")),
                _ => (),
            }
            notifier.await;
        }
    }

    /// Waits for the function to be unbound from a UDC.
    #[cfg(feature = "tokio")]
    pub async fn unbound(&self) {
        loop {
            let notifier = self.0.notify.notified();
            if self.state() != State::Bound {
                return;
            }
            notifier.await;
        }
    }

    /// The USB gadget function directory in configfs, if registered.
    pub fn path(&self) -> Option<PathBuf> {
        self.0.inner.lock().unwrap().dir.clone()
    }
}

/// USB gadget function directory container.
///
/// Stores the directory in configfs of a USB function and provides access methods.
#[derive(Clone)]
pub struct FunctionDir {
    inner: Arc<Mutex<FunctionDirInner>>,
    #[cfg(feature = "tokio")]
    notify: Arc<tokio::sync::Notify>,
}

#[derive(Debug, Default)]
struct FunctionDirInner {
    dir: Option<PathBuf>,
    dir_was_set: bool,
    bound: bool,
}

impl fmt::Debug for FunctionDir {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("FunctionDir").field(&*self.inner.lock().unwrap()).finish()
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
        Self {
            inner: Arc::new(Mutex::new(FunctionDirInner::default())),
            #[cfg(feature = "tokio")]
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub(crate) fn set_dir(&self, function_dir: &Path) {
        let mut inner = self.inner.lock().unwrap();
        inner.dir = Some(function_dir.to_path_buf());
        inner.dir_was_set = true;

        #[cfg(feature = "tokio")]
        self.notify.notify_waiters();
    }

    pub(crate) fn reset_dir(&self) {
        self.inner.lock().unwrap().dir = None;

        #[cfg(feature = "tokio")]
        self.notify.notify_waiters();
    }

    pub(crate) fn set_bound(&self, bound: bool) {
        self.inner.lock().unwrap().bound = bound;

        #[cfg(feature = "tokio")]
        self.notify.notify_waiters();
    }

    /// Create status accessor.
    pub fn status(&self) -> Status {
        Status(self.clone())
    }

    /// The USB gadget function directory in configfs.
    pub fn dir(&self) -> Result<PathBuf> {
        self.inner
            .lock()
            .unwrap()
            .dir
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
    pub fn create_dir(&self, name: impl AsRef<Path>) -> Result<()> {
        let path = self.property_path(name)?;
        log::debug!("creating directory {}", path.display());
        fs::create_dir(path)
    }

    /// Create a subdirectory and its parent directories.
    pub fn create_dir_all(&self, name: impl AsRef<Path>) -> Result<()> {
        let path = self.property_path(name)?;
        log::debug!("creating directories {}", path.display());
        fs::create_dir_all(path)
    }

    /// Remove a subdirectory.
    pub fn remove_dir(&self, name: impl AsRef<Path>) -> Result<()> {
        let path = self.property_path(name)?;
        log::debug!("removing directory {}", path.display());
        fs::remove_dir(path)
    }

    /// Read a binary property.
    pub fn read(&self, name: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = self.property_path(name)?;
        let res = fs::read(&path);

        match &res {
            Ok(value) => {
                log::debug!("read property {} with value {}", path.display(), String::from_utf8_lossy(value))
            }
            Err(err) => log::debug!("reading property {} failed: {}", path.display(), err),
        }

        res
    }

    /// Read and trim a string property.
    pub fn read_string(&self, name: impl AsRef<Path>) -> Result<String> {
        let mut data = self.read(name)?;
        while data.last() == Some(&b'\0') || data.last() == Some(&b' ') || data.last() == Some(&b'\n') {
            data.truncate(data.len() - 1);
        }

        Ok(String::from_utf8(data).map_err(|err| Error::new(ErrorKind::InvalidData, err))?.trim().to_string())
    }

    /// Read an trim an OS string property.
    pub fn read_os_string(&self, name: impl AsRef<Path>) -> Result<OsString> {
        Ok(trim_os_str(&OsString::from_vec(self.read(name)?)).to_os_string())
    }

    /// Write a property.
    pub fn write(&self, name: impl AsRef<Path>, value: impl AsRef<[u8]>) -> Result<()> {
        let path = self.property_path(name)?;
        let value = value.as_ref();
        log::debug!("setting property {} to {}", path.display(), String::from_utf8_lossy(value));
        fs::write(path, value)
    }

    /// Create a symbolic link.
    pub fn symlink(&self, target: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
        let target = self.property_path(target)?;
        let link = self.property_path(link)?;
        log::debug!("creating symlink {} -> {}", link.display(), target.display());
        std::os::unix::fs::symlink(target, link)
    }
}

/// Split configfs function directory path into driver name and instance name.
pub fn split_function_dir(function_dir: &Path) -> Option<(&OsStr, &OsStr)> {
    let name = function_dir.file_name()?;
    let name = name.as_bytes();

    let dot = name.iter().enumerate().find_map(|(i, c)| if *c == b'.' { Some(i) } else { None })?;
    let driver = &name[..dot];
    let instance = &name[dot + 1..];

    Some((OsStr::from_bytes(driver), OsStr::from_bytes(instance)))
}

/// Handler function for removing function instance.
type RemoveHandler = Arc<dyn Fn(PathBuf) -> Result<()> + Send + Sync>;

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
pub fn register_remove_handler(
    driver: impl AsRef<OsStr>, handler: impl Fn(PathBuf) -> Result<()> + Send + Sync + 'static,
) {
    remove_handlers().insert(driver.as_ref().to_os_string(), Arc::new(handler));
}

/// Calls the remove handler for the function directory, if any is registered.
pub(crate) fn call_remove_handler(function_dir: &Path) -> Result<()> {
    let Some((driver, _)) = split_function_dir(function_dir) else {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid function directory"));
    };

    let handler_opt = remove_handlers().get(driver).cloned();
    match handler_opt {
        Some(handler) => handler(function_dir.to_path_buf()),
        None => Ok(()),
    }
}

/// Value channel.
pub(crate) mod value {
    use std::{
        error::Error,
        fmt,
        fmt::Display,
        io, mem,
        sync::{mpsc, Mutex},
    };

    /// Value was already sent.
    #[derive(Debug, Clone)]
    pub struct AlreadySentError;

    impl Display for AlreadySentError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "value already sent")
        }
    }

    impl Error for AlreadySentError {}

    /// Sender of value channel.
    #[derive(Debug)]
    pub struct Sender<T>(Mutex<Option<mpsc::Sender<T>>>);

    impl<T> Sender<T> {
        /// Sends a value.
        ///
        /// This can only be called once.
        pub fn send(&self, value: T) -> Result<(), AlreadySentError> {
            match self.0.lock().unwrap().take() {
                Some(tx) => {
                    let _ = tx.send(value);
                    Ok(())
                }
                None => Err(AlreadySentError),
            }
        }
    }

    /// Value channel receive error.
    #[derive(Debug, Clone)]
    pub enum RecvError {
        /// Value was not yet sent.
        Empty,
        /// Sender was dropped without sending a value.
        Disconnected,
        /// Value was taken.
        Taken,
    }

    impl Display for RecvError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self {
                RecvError::Empty => write!(f, "value was not yet sent"),
                RecvError::Disconnected => write!(f, "no value was sent"),
                RecvError::Taken => write!(f, "value was taken"),
            }
        }
    }

    impl Error for RecvError {}

    impl From<mpsc::RecvError> for RecvError {
        fn from(_err: mpsc::RecvError) -> Self {
            Self::Disconnected
        }
    }

    impl From<mpsc::TryRecvError> for RecvError {
        fn from(err: mpsc::TryRecvError) -> Self {
            match err {
                mpsc::TryRecvError::Empty => Self::Empty,
                mpsc::TryRecvError::Disconnected => Self::Disconnected,
            }
        }
    }

    impl From<RecvError> for io::Error {
        fn from(err: RecvError) -> Self {
            match err {
                RecvError::Empty => io::Error::new(io::ErrorKind::WouldBlock, err),
                RecvError::Disconnected => io::Error::new(io::ErrorKind::BrokenPipe, err),
                RecvError::Taken => io::Error::new(io::ErrorKind::Other, err),
            }
        }
    }

    /// Receiver state.
    #[derive(Default)]
    enum State<T> {
        Receiving(Mutex<mpsc::Receiver<T>>),
        Received(T),
        #[default]
        Taken,
    }

    /// Receiver of value channel.
    #[derive(Default)]
    pub struct Receiver<T>(State<T>);

    impl<T: fmt::Debug> fmt::Debug for Receiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match &self.0 {
                State::Receiving(_) => write!(f, "<uninit>"),
                State::Received(v) => v.fmt(f),
                State::Taken => write!(f, "<taken>"),
            }
        }
    }

    impl<T> Receiver<T> {
        /// Get the value, if it has been sent.
        pub fn get(&mut self) -> Result<&mut T, RecvError> {
            match &mut self.0 {
                State::Receiving(rx) => {
                    let value = rx.get_mut().unwrap().try_recv()?;
                    self.0 = State::Received(value);
                }
                State::Taken => return Err(RecvError::Taken),
                _ => (),
            }

            let State::Received(value) = &mut self.0 else { unreachable!() };
            Ok(value)
        }

        /// Wait for the value.
        #[allow(dead_code)]
        pub fn wait(&mut self) -> Result<&mut T, RecvError> {
            if let State::Receiving(rx) = &mut self.0 {
                let value = rx.get_mut().unwrap().recv()?;
                self.0 = State::Received(value);
            }

            self.get()
        }

        /// Take the value, if it has been sent.
        #[allow(dead_code)]
        pub fn take(&mut self) -> Result<T, RecvError> {
            self.get()?;

            let State::Received(value) = mem::take(&mut self.0) else { unreachable!() };
            Ok(value)
        }
    }

    /// Creates a new value channel.
    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = mpsc::channel();
        (Sender(Mutex::new(Some(tx))), Receiver(State::Receiving(Mutex::new(rx))))
    }
}
