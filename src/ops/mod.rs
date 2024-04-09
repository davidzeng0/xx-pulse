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
async fn internal_get_pulse_env() -> Ptr<Pulse> {
	let context = get_context().await;

	/* Safety: we are in an async function */
	let env = unsafe { ptr!(context=>get_environment::<Pulse>()) };

	#[allow(clippy::unwrap_used)]
	env.ok_or_else(|| fmt_error!("Cannot use xx-pulse functions with a different runtime"))
		.unwrap()
		.into()
}

#[asynchronous]
async fn internal_get_driver() -> Ptr<Driver> {
	let env = internal_get_pulse_env().await;

	/* Safety: environment outlives context */
	unsafe { ptr!(env=>driver) }
}
