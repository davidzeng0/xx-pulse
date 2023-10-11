use xx_core::task::env::Handle;

use crate::{async_runtime::*, driver::Driver};

#[async_fn]
async fn internal_get_context() -> Handle<Context> {
	__xx_async_internal_context
}

#[async_fn]
async fn internal_get_driver() -> Handle<Driver> {
	internal_get_context().await.driver()
}

pub mod io;
pub mod select;
pub mod spawn;
pub mod timers;
