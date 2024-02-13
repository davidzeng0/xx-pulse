mod file;
mod socket;

use std::{
	io::{IoSlice, IoSliceMut},
	os::fd::{AsFd, BorrowedFd, OwnedFd}
};

pub use file::*;
pub use socket::*;
use xx_core::{async_std::io::*, error::*, os::iovec::*, read_into, write_from};

use super::*;
