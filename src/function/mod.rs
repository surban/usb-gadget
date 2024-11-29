//! USB gadget functions.

pub mod custom;
pub mod hid;
pub mod midi;
pub mod msd;
pub mod net;
pub mod other;
pub mod serial;
pub mod util;

use std::{cmp, hash, hash::Hash, sync::Arc};

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
        Some(self.cmp(other))
    }
}

impl Ord for Handle {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        Arc::as_ptr(&self.0).cast::<()>().cmp(&Arc::as_ptr(&other.0).cast::<()>())
    }
}

impl Hash for Handle {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// Register included remove handlers.
fn register_remove_handlers() {
    register_remove_handler(custom::driver(), custom::remove_handler);
    register_remove_handler(msd::driver(), msd::remove_handler);
}
