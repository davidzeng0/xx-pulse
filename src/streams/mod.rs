use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use xx_core::async_std::io::*;
use xx_core::error::*;
use xx_core::os::iovec::*;

use super::*;

pub mod file;
pub mod socket;

pub use file::*;
pub use socket::*;
