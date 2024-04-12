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
#[context('current)]
async fn internal_get_pulse_env() -> &'current Pulse {
	/* Safety: we are in an async function */
	let env = unsafe { get_context().await }.get_environment::<Pulse>();

	#[allow(clippy::unwrap_used)]
	env.ok_or_else(|| fmt_error!("Cannot use xx-pulse functions with a different runtime"))
		.unwrap()
}

#[asynchronous]
#[context('current)]
async fn internal_get_driver() -> &'current Driver {
	let env = internal_get_pulse_env().await;

	/* Safety: driver outlives context */
	unsafe { env.driver.as_ref() }
}
