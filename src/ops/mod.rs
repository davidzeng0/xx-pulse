use xx_core::error::*;

use crate::{driver::Driver, *};

mod io;
mod join;
mod select;
mod spawn;
mod timers;

pub use io::*;
pub use join::*;
pub use select::*;
pub use spawn::*;
pub use timers::*;
use xx_core::coroutines::block_on;
pub use xx_core::coroutines::{Join, JoinHandle, Select};

#[async_fn]
async fn internal_get_runtime_context() -> Handle<RuntimeContext> {
	get_context()
		.await
		.get_runtime::<RuntimeContext>()
		.ok_or_else(|| {
			Error::new(
				ErrorKind::Other,
				"Cannot use xx-pulse functions with a different runtime"
			)
		})
		.unwrap()
}

#[async_fn]
async fn internal_get_driver() -> Handle<Driver> {
	internal_get_runtime_context().await.driver()
}
