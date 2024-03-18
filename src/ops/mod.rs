use xx_core::{coroutines::block_on, pointer::*};

use super::*;

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
pub use xx_core::coroutines::{Join, JoinHandle, Select};

#[asynchronous]
async fn internal_get_runtime_context() -> Ptr<Pulse> {
	/* Safety: we are in an async function */
	let env = unsafe { get_context().await.as_ref() }.get_environment::<Pulse>();

	#[allow(clippy::unwrap_used)]
	env.ok_or_else(|| {
		Error::simple(
			ErrorKind::Other,
			Some("Cannot use xx-pulse functions with a different runtime")
		)
	})
	.unwrap()
	.into()
}

#[asynchronous]
async fn internal_get_driver() -> Ptr<Driver> {
	/* Safety: environment outlives context */
	unsafe { internal_get_runtime_context().await.as_ref() }.driver()
}
