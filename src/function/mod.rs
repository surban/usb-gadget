//! USB gadget functions.

use std::{cmp, hash, hash::Hash, sync::Arc};

mod custom;
pub use custom::*;

mod hid;
pub use hid::*;

mod msd;
pub use msd::*;

mod net;
pub use net::*;

mod other;
pub use other::*;

mod serial;
pub use serial::*;

pub mod util;

use self::util::{register_remove_handler, Function};

/// USB gadget function handle.
///
/// Use a member of the [function module](crate::function) to obtain a
/// gadget function handle.
#[derive(Debug, Clone)]
pub struct Handle(Arc<dyn Function>);

impl Handle {
    pub(crate) fn new<F: Function>(f: F) -> Self {
        Self(Arc::new(f))
    }
}

impl Handle {
    pub(crate) fn get(&self) -> &dyn Function {
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

/// Register included remove handlers.
fn register_remove_handlers() {
    register_remove_handler(msd::driver(), msd::remove_handler);
}
