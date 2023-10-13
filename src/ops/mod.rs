use xx_core::{coroutines::runtime::get_context, task::env::Handle};

use crate::{async_runtime::*, driver::Driver};

pub mod io;
pub mod join;
pub mod select;
pub mod spawn;
pub mod timers;

#[async_fn]
async fn internal_get_context() -> Handle<Context> {
	get_context().await
}

#[async_fn]
async fn internal_get_driver() -> Handle<Driver> {
	internal_get_context().await.driver()
}
