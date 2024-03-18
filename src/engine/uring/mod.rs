#![allow(unreachable_pub, clippy::module_name_repetitions)]

use std::mem::size_of;

use xx_core::os::{io_uring::*, stat::*, time::*};

mod engine;
mod op;

pub use engine::IoUring;
use op::*;

use super::*;
