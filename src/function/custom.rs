use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{Error, ErrorKind, Result},
    path::{Component, Path, PathBuf},
};
use tokio::fs;

use crate::check_name;

/// Custom USB interface, implemented in user code.
#[derive(Debug, Clone)]
pub struct CustomBuilder {}
