use std::{
	io::{IoSlice, IoSliceMut},
	os::fd::{AsFd, BorrowedFd, OwnedFd}
};

use xx_core::{async_std::io::*, error::*, os::iovec::*};

use super::*;

pub mod file;
pub mod socket;

pub use file::*;
pub use socket::*;
