//! Linux AIO driver.

use bytes::{Bytes, BytesMut};
use nix::sys::eventfd::{eventfd, EfdFlags};
use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    fmt,
    io::{Error, ErrorKind, Result},
    mem::{self, MaybeUninit},
    ops::Deref,
    os::fd::{AsRawFd, OwnedFd, RawFd},
    pin::Pin,
    ptr,
    sync::{mpsc, mpsc::TryRecvError, Arc},
    thread,
    time::Duration,
};

mod sys;

pub use sys::opcode;

/// eventfd provided by kernel.
#[derive(Debug, Clone)]
struct EventFd(Arc<OwnedFd>);

impl EventFd {
    /// Create new eventfd with initial value and semaphore characteristics, if requested.
    pub fn new(initval: u32, semaphore: bool) -> Result<Self> {
        let flags = if semaphore { EfdFlags::EFD_SEMAPHORE } else { EfdFlags::empty() };
        let fd = eventfd(initval, flags)?;
        Ok(Self(Arc::new(fd)))
    }

    /// Decrease value by one or set to zero if using semaphore characteristics.
    ///
    /// Blocks while value is zero.
    pub fn read(&self) -> Result<u64> {
        let mut buf = [0; 8];
        let ret = unsafe { libc::read(self.0.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if ret != buf.len() as _ {
            return Err(Error::last_os_error());
        }

        Ok(u64::from_ne_bytes(buf))
    }

    /// Increase value by `n`.
    pub fn write(&self, n: u64) -> Result<()> {
        let buf = n.to_ne_bytes();
        let ret = unsafe { libc::write(self.0.as_raw_fd(), buf.as_ptr() as *mut _, buf.len()) };
        if ret != buf.len() as _ {
            return Err(Error::last_os_error());
        }
        Ok(())
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

/// AIO context wrapper.
#[derive(Debug)]
struct Context(sys::ContextId);

impl Context {
    /// create an asynchronous I/O context
    pub fn new(nr_events: u32) -> Result<Self> {
        let mut id = 0;
        unsafe { sys::setup(nr_events, &mut id) }?;
        Ok(Self(id))
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { sys::destroy(self.0) }.expect("cannot destory AIO context");
    }
}

impl Deref for Context {
    type Target = sys::ContextId;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Data buffer for AIO operation.
#[derive(Debug)]
pub enum Buffer {
    /// Initialized buffer for writing data.
    Write(Bytes),
    /// Possibly uninitialized buffer for reading data.
    Read(BytesMut),
}

impl Buffer {
    /// Length or capacity of buffer.
    pub fn size(&self) -> usize {
        match self {
            Self::Write(buf) => buf.len(),
            Self::Read(buf) => buf.capacity(),
        }
    }

    /// Get pointer to buffer.
    ///
    /// ## Safety
    /// If this is a write buffer the pointer must only be read from.
    unsafe fn as_mut_ptr(&mut self) -> *mut u8 {
        match self {
            Self::Write(buf) => buf.as_ptr() as *mut _,
            Self::Read(buf) => buf.as_mut_ptr(),
        }
    }

    /// Assume buffer is initialized to given length.
    unsafe fn assume_init(&mut self, len: usize) {
        match self {
            Self::Write(_) => (),
            Self::Read(buf) => buf.set_len(len),
        }
    }
}

impl From<Bytes> for Buffer {
    fn from(buf: Bytes) -> Self {
        Self::Write(buf)
    }
}

impl From<BytesMut> for Buffer {
    fn from(buf: BytesMut) -> Self {
        Self::Read(buf)
    }
}

impl From<Buffer> for Bytes {
    fn from(buf: Buffer) -> Self {
        match buf {
            Buffer::Write(buf) => buf,
            Buffer::Read(buf) => buf.freeze(),
        }
    }
}

/// Buffer is not a read buffer.
#[derive(Debug, Clone)]
pub struct NotAReadBuffer;

impl TryFrom<Buffer> for BytesMut {
    type Error = NotAReadBuffer;
    fn try_from(buf: Buffer) -> std::result::Result<Self, NotAReadBuffer> {
        match buf {
            Buffer::Write(_) => Err(NotAReadBuffer),
            Buffer::Read(buf) => Ok(buf),
        }
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::Write(Bytes::new())
    }
}

/// AIO operation.
struct Op {
    /// IO control block.
    pub iocb: Pin<Box<sys::IoCb>>,
    /// Buffer referenced by [`Self::iocb`].
    pub buf: Buffer,
}

impl Default for Op {
    fn default() -> Self {
        Self { iocb: Box::pin(Default::default()), buf: Default::default() }
    }
}

impl Op {
    /// Get pointer to IO control block.
    fn iocb_ptr(&mut self) -> *mut sys::IoCb {
        Pin::into_inner(self.iocb.as_mut()) as *mut _
    }

    /// Given received AIO event convert operation to result.
    fn complete(mut self, event: sys::IoEvent) -> CompletedOp {
        assert_eq!(event.data, self.iocb.data);

        let result = if event.res >= 0 {
            unsafe { self.buf.assume_init(event.res.try_into().unwrap()) };
            Ok(self.buf)
        } else {
            Err(Error::from_raw_os_error(-i32::try_from(event.res).unwrap()))
        };

        CompletedOp { id: event.data, res: event.res, res2: event.res2, result }
    }
}

/// AIO operation handle.
pub struct OpHandle(u64);

impl OpHandle {
    /// Operation id.
    #[allow(dead_code)]
    pub const fn id(&self) -> u64 {
        self.0
    }
}

/// Completed AIO operation.
#[derive(Debug)]
pub struct CompletedOp {
    id: u64,
    res: i64,
    res2: i64,
    result: Result<Buffer>,
}

impl CompletedOp {
    /// Operation id.
    #[allow(dead_code)]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Retrieve result.
    pub fn result(self) -> Result<Buffer> {
        self.result
    }

    /// Result code.
    #[allow(dead_code)]
    pub const fn res(&self) -> i64 {
        self.res
    }

    /// Result code 2.
    #[allow(dead_code)]
    pub const fn res2(&self) -> i64 {
        self.res2
    }
}

enum Cmd {
    Insert(Op),
    Remove(u64),
    #[allow(dead_code)]
    Cancel(u64),
    CancelAll,
}

#[cfg(feature = "tokio")]
type TNotify = Arc<tokio::sync::Notify>;
#[cfg(not(feature = "tokio"))]
type TNotify = Arc<()>;

/// AIO driver.
///
/// All outstanding operations are cancelled when this is dropped.
pub struct Driver {
    aio: Arc<Context>,
    cmd_tx: mpsc::Sender<Cmd>,
    done_rx: mpsc::Receiver<CompletedOp>,
    next_id: u64,
    eventfd: EventFd,
    space: u32,
    queue_length: u32,
    #[cfg(feature = "tokio")]
    notify: Arc<tokio::sync::Notify>,
}

impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Driver")
            .field("aio", &*self.aio)
            .field("next_id", &self.next_id)
            .field("space", &self.space)
            .field("queue_length", &self.queue_length)
            .finish()
    }
}

impl Driver {
    /// Create new AIO driver.
    pub fn new(queue_length: u32, thread_name: Option<String>) -> Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        let aio = Arc::new(Context::new(queue_length)?);
        let eventfd = EventFd::new(0, true)?;

        #[cfg(feature = "tokio")]
        let notify = Arc::new(tokio::sync::Notify::new());
        #[cfg(not(feature = "tokio"))]
        let notify = Arc::new(());

        let aio_thread = aio.clone();
        let eventfd_thread = eventfd.clone();
        let notify_thread = notify.clone();

        let mut builder = thread::Builder::new();
        if let Some(thread_name) = thread_name {
            builder = builder.name(thread_name);
        }
        builder.spawn(|| Self::thread(aio_thread, eventfd_thread, cmd_rx, done_tx, notify_thread))?;

        Ok(Self {
            aio,
            cmd_tx,
            done_rx,
            next_id: 0,
            eventfd,
            space: queue_length,
            queue_length,
            #[cfg(feature = "tokio")]
            notify,
        })
    }

    /// Returns whether the queue of AIO operations is full.
    pub fn is_full(&self) -> bool {
        self.space == 0
    }

    /// Returns whether the queue of AIO operations is empty.
    pub fn is_empty(&self) -> bool {
        self.space == self.queue_length
    }

    /// Submits an AIO operation.
    pub fn submit(&mut self, opcode: u16, file: impl AsRawFd, buf: impl Into<Buffer>) -> Result<OpHandle> {
        if self.is_full() {
            return Err(Error::new(ErrorKind::WouldBlock, "no AIO queue space available"));
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let mut buf = buf.into();
        let iocb =
            sys::IoCb::new(opcode, file.as_raw_fd(), unsafe { buf.as_mut_ptr() }, buf.size().try_into().unwrap())
                .with_resfd(self.eventfd.as_raw_fd())
                .with_data(id);

        let mut op = Op { iocb: Box::pin(iocb), buf };
        let iocb_ptr = op.iocb_ptr();
        self.cmd_tx.send(Cmd::Insert(op)).unwrap();

        let mut iocbs = [iocb_ptr];
        match unsafe { sys::submit(**self.aio, 1, iocbs.as_mut_ptr()) } {
            Ok(1) => {
                self.space -= 1;
                self.eventfd.write(1).unwrap();
                Ok(OpHandle(id))
            }
            res => {
                self.cmd_tx.send(Cmd::Remove(id)).unwrap();
                self.eventfd.write(1).unwrap();

                match res {
                    Ok(_) => Err(Error::new(ErrorKind::WouldBlock, "AIO request not accepted")),
                    Err(err) => Err(err),
                }
            }
        }
    }

    /// Retrieves the next operation from the completion queue.
    ///
    /// Blocks until a completed operation becomes available.
    pub fn completed(&mut self) -> Option<CompletedOp> {
        if self.is_empty() {
            return None;
        }

        let res = self.done_rx.recv().unwrap();
        self.space += 1;
        Some(res)
    }

    /// Asynchronously retrieves the next operation from the completion queue.
    ///
    /// Waits until a completed operation becomes available.
    #[cfg(feature = "tokio")]
    pub async fn wait_completed(&mut self) -> Option<CompletedOp> {
        if self.is_empty() {
            return None;
        }

        loop {
            if let Some(op) = self.try_completed() {
                return Some(op);
            }

            self.notify.notified().await;
        }
    }

    /// Retrieves the next operation from the completion queue with a timeout.
    ///
    /// Blocks until a completed operation becomes available or the timeout is reached.
    pub fn completed_timeout(&mut self, timeout: Duration) -> Option<CompletedOp> {
        if self.is_empty() {
            return None;
        }

        let res = self.done_rx.recv_timeout(timeout).ok();
        if res.is_some() {
            self.space += 1;
        }
        res
    }

    /// Retrieves the next operation from the completion queue without blocking.
    ///
    /// Returns immediately if no completed operation is available.
    pub fn try_completed(&mut self) -> Option<CompletedOp> {
        let res = self.done_rx.try_recv().ok();
        if res.is_some() {
            self.space += 1;
        }
        res
    }

    /// Requests cancellation of the specified operation.
    #[allow(dead_code)]
    pub fn cancel(&mut self, handle: OpHandle) {
        self.cmd_tx.send(Cmd::Cancel(handle.0)).unwrap();
        self.eventfd.write(1).unwrap();
    }

    /// Requests cancellation of all operations.
    pub fn cancel_all(&mut self) {
        self.cmd_tx.send(Cmd::CancelAll).unwrap();
        self.eventfd.write(1).unwrap();
    }

    /// Thread managing submitted AIO operations.
    fn thread(
        aio: Arc<Context>, eventfd: EventFd, cmd_rx: mpsc::Receiver<Cmd>, done_tx: mpsc::Sender<CompletedOp>,
        notify: TNotify,
    ) {
        #[cfg(not(feature = "tokio"))]
        let _ = notify;

        let mut active: HashMap<u64, Op> = HashMap::new();
        let mut event_queue = VecDeque::new();

        'outer: loop {
            // Wait for event.
            eventfd.read().unwrap();

            // Process commands.
            loop {
                match cmd_rx.try_recv() {
                    Ok(Cmd::Insert(op)) => {
                        if active.insert(op.iocb.data, op).is_some() {
                            panic!("submitted aio request with duplicate id");
                        }
                    }
                    Ok(Cmd::Remove(id)) => {
                        active.remove(&id).expect("received remove request for unknown id");
                    }
                    Ok(Cmd::Cancel(id)) => {
                        if let Entry::Occupied(mut op) = active.entry(id) {
                            let mut event = MaybeUninit::<sys::IoEvent>::uninit();
                            if unsafe {
                                sys::cancel(**aio, op.get_mut().iocb_ptr(), &mut event as *mut _ as *mut _)
                            }
                            .is_ok()
                            {
                                let _ = done_tx.send(op.remove().complete(unsafe { event.assume_init() }));
                                #[cfg(feature = "tokio")]
                                notify.notify_one();
                            }
                        }
                    }
                    Ok(Cmd::CancelAll) => {
                        active.retain(|_id, op| {
                            let mut event = MaybeUninit::<sys::IoEvent>::uninit();
                            if unsafe { sys::cancel(**aio, op.iocb_ptr(), &mut event as *mut _ as *mut _) }
                                .is_ok()
                            {
                                let _ = done_tx.send(mem::take(op).complete(unsafe { event.assume_init() }));
                                #[cfg(feature = "tokio")]
                                notify.notify_one();
                                false
                            } else {
                                true
                            }
                        });
                    }
                    Err(TryRecvError::Disconnected) if active.is_empty() => break 'outer,
                    Err(_) => break,
                }
            }

            // Fetch AIO events.
            loop {
                let mut events = [MaybeUninit::<sys::IoEvent>::uninit(); 16];

                let n = unsafe {
                    sys::getevents(**aio, 0, events.len() as _, events.as_mut_ptr() as *mut _, ptr::null())
                }
                .expect("io_getevents failed");

                if n == 0 {
                    break;
                }

                for event in events.into_iter().take(n.try_into().unwrap()) {
                    let event = unsafe { event.assume_init() };
                    event_queue.push_back(event);
                }
            }

            // Process AIO events.
            while let Some(event) = event_queue.front() {
                match active.remove(&event.data) {
                    Some(op) => {
                        let _ = done_tx.send(op.complete(event_queue.pop_front().unwrap()));
                        #[cfg(feature = "tokio")]
                        notify.notify_one();
                    }
                    None => break,
                }
            }
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
