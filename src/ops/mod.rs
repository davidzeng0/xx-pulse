use xx_core::{
	error::Result,
	task::{Cancel, Handle, Task}
};

use crate::{driver::Driver, *};

pub mod io;
pub mod join;
pub mod select;
pub mod spawn;
pub mod timers;

pub use io::*;
pub use join::*;
pub use select::*;
pub use spawn::*;
pub use timers::*;

#[async_fn]
async fn internal_get_context() -> Handle<Context> {
	get_context().await
}

#[async_fn]
async fn internal_get_driver() -> Handle<Driver> {
	internal_get_context().await.driver()
}

#[async_fn]
#[inline(always)]
pub async fn try_block_on<T: Task<Result<Output>, C>, C: Cancel, Output>(
	task: T
) -> Result<Output> {
	check_interrupt().await?;
	get_context().await.block_on(task)
}
