use std::time::Duration;

use enumflags2::BitFlags;
use xx_core::{coroutines::runtime::*, error::Result};

use super::*;
use crate::driver::TimeoutFlag;

#[async_fn]
#[inline(always)]
pub async fn timeout(expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.timeout(expire, flags)).await
}

#[async_fn]
#[inline(always)]
pub async fn sleep(duration: Duration) -> Result<()> {
	timeout(duration.as_nanos() as u64, BitFlags::default()).await
}
