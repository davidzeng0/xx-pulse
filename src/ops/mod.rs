use xx_core::{
	coroutines::{block_on, get_context},
	pointer::*
};

use super::*;

pub mod branch;
pub mod io;
pub mod timers;

pub use branch::*;
pub use timers::*;
pub use xx_core::coroutines::{Join, JoinHandle, Select};

#[asynchronous]
async fn internal_get_runtime_context() -> Ptr<Pulse> {
	/* Safety: we are in an async function */
	let env = unsafe { get_context().await.as_ref() }.get_environment::<Pulse>();

	#[allow(clippy::unwrap_used)]
	env.ok_or_else(|| fmt_error!("Cannot use xx-pulse functions with a different runtime"))
		.unwrap()
		.into()
}

#[asynchronous]
async fn internal_get_driver() -> Ptr<Driver> {
	/* Safety: environment outlives context */
	unsafe { internal_get_runtime_context().await.as_ref() }.driver()
}
