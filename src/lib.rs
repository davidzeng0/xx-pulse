use std::time::Duration;

use enumflags2::BitFlags;
use xx_core::coroutines::{self, Context, Environment, Executor, Task};
use xx_core::error::*;
use xx_core::future::{self, future, Future, Progress, ReqPtr, Request};

mod driver;
mod engine;
pub mod impls;
pub mod interval;
pub mod macros;
pub mod ops;
mod runtime;
pub mod streams;

pub use driver::TimeoutFlag;
use driver::*;
use engine::*;
pub use interval::*;
pub use macros::*;
pub use ops::*;
pub use runtime::Runtime;
use runtime::*;
pub use streams::*;
pub use xx_core::coroutines::{
	acquire_budget, asynchronous, block_on, check_interrupt, check_interrupt_take, current_budget,
	get_context, interrupt_guard, is_interrupted, scoped, take_interrupt
};
