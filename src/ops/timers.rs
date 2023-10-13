use std::{io::Result, time::Duration};

use enumflags2::BitFlags;
use xx_core::coroutines::task::BlockOn;

use super::*;
use crate::driver::TimeoutFlag;

#[async_fn]
pub async fn timeout(expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
	BlockOn::new(internal_get_driver().await.timeout(expire, flags)).await
}

#[async_fn]
pub async fn sleep(duration: Duration) -> Result<()> {
	timeout(duration.as_nanos() as u64, BitFlags::default()).await
}