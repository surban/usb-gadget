//! Additional ioctl patterns for use with [`rustix::ioctl`].

use rustix::ioctl::{Ioctl, IoctlOutput, Opcode};
use std::ffi::{c_int, c_void};

/// Ioctl that takes no pointer argument and returns the raw return value.
///
/// This is useful for ioctls that encode their result in the syscall return
/// value rather than writing through a pointer, such as FunctionFS ioctls.
pub(crate) struct ReturnInt<const OPCODE: Opcode> {}

impl<const OPCODE: Opcode> ReturnInt<OPCODE> {
    /// # Safety
    /// Opcode must be valid for the target file descriptor.
    pub unsafe fn new() -> Self {
        Self {}
    }
}

unsafe impl<const OPCODE: Opcode> Ioctl for ReturnInt<OPCODE> {
    type Output = c_int;
    const IS_MUTATING: bool = false;

    fn opcode(&self) -> Opcode {
        OPCODE
    }

    fn as_ptr(&mut self) -> *mut c_void {
        core::ptr::null_mut()
    }

    unsafe fn output_from_ptr(out: IoctlOutput, _: *mut c_void) -> rustix::io::Result<Self::Output> {
        Ok(out)
    }
}

/// Ioctl that passes an integer argument and returns the raw return value.
///
/// This is useful for ioctls that take a small integer value directly (not via
/// pointer) and encode their result in the syscall return value.
pub(crate) struct IntReturnInt<const OPCODE: Opcode> {
    value: usize,
}

impl<const OPCODE: Opcode> IntReturnInt<OPCODE> {
    /// # Safety
    /// Opcode and value must be valid for the target file descriptor.
    pub unsafe fn new(value: usize) -> Self {
        Self { value }
    }
}

unsafe impl<const OPCODE: Opcode> Ioctl for IntReturnInt<OPCODE> {
    type Output = c_int;
    const IS_MUTATING: bool = false;

    fn opcode(&self) -> Opcode {
        OPCODE
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.value as *mut c_void
    }

    unsafe fn output_from_ptr(out: IoctlOutput, _: *mut c_void) -> rustix::io::Result<Self::Output> {
        Ok(out)
    }
}
