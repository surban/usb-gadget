//! Linux-native AIO interface.

use libc::{
    c_int, c_long, c_uint, c_ulong, syscall, timespec, SYS_io_cancel, SYS_io_destroy, SYS_io_getevents,
    SYS_io_setup, SYS_io_submit,
};
use std::{
    io::{Error, Result},
    os::fd::RawFd,
};

/// AIO context.
pub type ContextId = c_ulong;

/// Opcodes for [`IoCb::opcode`].
#[allow(dead_code)]
pub mod opcode {
    pub const PREAD: u16 = 0;
    pub const PWRITE: u16 = 1;
    pub const FSYNC: u16 = 2;
    pub const FDSYNC: u16 = 3;
    pub const POLL: u16 = 5;
    pub const NOOP: u16 = 6;
    pub const PREADV: u16 = 7;
    pub const PWRITEV: u16 = 8;
}

/// Validity flags for [`IoCb`].
#[allow(dead_code)]
pub mod flags {
    /// Set if [`IoCb::resfd`](super::IoCb::resfd) is valid.
    pub const RESFD: u32 = 1 << 0;
    /// Set if [`IoCb::reqprio`](super::IoCb::reqprio) is valid.
    pub const IOPRIO: u32 = 1 << 1;
}

/// AIO event.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IoEvent {
    /// data from [`IoCb::data`]
    pub data: u64,
    /// what iocb this event came from
    pub obj: u64,
    /// result code for this event
    pub res: i64,
    /// secondary result
    pub res2: i64,
}

/// AIO control block.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IoCb {
    /// data to be returned in [`IoEvent::data`].
    pub data: u64,

    /// the kernel sets key to the req #
    #[cfg(target_endian = "little")]
    pub key: u32,
    /// RWF_* flags
    #[cfg(target_endian = "little")]
    pub rw_flags: c_int,

    /// RWF_* flags
    #[cfg(target_endian = "big")]
    pub rw_flags: c_int,
    /// the kernel sets key to the req #
    #[cfg(target_endian = "big")]
    pub key: u32,

    /// Opcode from [`opcode`].
    pub opcode: u16,

    /// This defines the requests priority.
    pub reqprio: i16,

    /// The file descriptor on which the I/O operation is to be performed.
    pub fildes: RawFd,

    /// This is the buffer used to transfer data for a read or write operation.
    pub buf: u64,

    /// This is the size of the buffer pointed to by [`buf`].
    pub nbytes: u64,

    /// This is the file offset at which the I/O operation is to be performed.
    pub offset: i64,

    pub _reserved2: u64,

    /// Flags specified by [`flags`].
    pub flags: u32,

    /// if the IOCB_FLAG_RESFD flag of "aio_flags" is set, this is an
    /// eventfd to signal AIO readiness to
    pub resfd: RawFd,
}

impl IoCb {
    pub fn new(opcode: u16, fildes: RawFd, buf: *mut u8, nbytes: u64) -> Self {
        Self { opcode, fildes, buf: buf as usize as u64, nbytes, ..Default::default() }
    }

    pub fn with_resfd(mut self, resfd: RawFd) -> Self {
        self.resfd = resfd;
        self.flags |= flags::RESFD;
        self
    }

    pub fn with_data(mut self, data: u64) -> Self {
        self.data = data;
        self
    }
}

/// create an asynchronous I/O context
pub unsafe fn setup(nr_events: c_uint, ctx_idp: &mut ContextId) -> Result<()> {
    match syscall(SYS_io_setup, nr_events, ctx_idp as *mut _) {
        0 => Ok(()),
        _ => Err(Error::last_os_error()),
    }
}

/// destroy an asynchronous I/O context
pub unsafe fn destroy(ctx_id: ContextId) -> Result<()> {
    match syscall(SYS_io_destroy, ctx_id) as c_int {
        0 => Ok(()),
        _ => Err(Error::last_os_error()),
    }
}

/// submit asynchronous I/O blocks for processing
pub unsafe fn submit(ctx_id: ContextId, nr: c_ulong, iocbpp: *mut *mut IoCb) -> Result<c_int> {
    match syscall(SYS_io_submit, ctx_id, nr, iocbpp) as c_int {
        -1 => Err(Error::last_os_error()),
        n => Ok(n),
    }
}

/// cancel an outstanding asynchronous I/O operation
pub unsafe fn cancel(ctx_id: ContextId, iocb: *mut IoCb, result: *mut IoEvent) -> Result<()> {
    match syscall(SYS_io_cancel, ctx_id, iocb, result) as c_int {
        0 => Ok(()),
        _ => Err(Error::last_os_error()),
    }
}

/// read asynchronous I/O events from the completion queue
pub unsafe fn getevents(
    ctx_id: ContextId, min_nr: c_long, nr: c_long, events: *mut IoEvent, timeout: *const timespec,
) -> Result<c_int> {
    match syscall(SYS_io_getevents, ctx_id, min_nr, nr, events, timeout) as c_int {
        -1 => Err(Error::last_os_error()),
        n => Ok(n),
    }
}
