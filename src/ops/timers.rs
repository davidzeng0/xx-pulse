use super::*;

#[asynchronous]
pub async fn timeout(expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
	let driver = internal_get_driver().await;

	check_interrupt().await?;
	block_on(driver.timeout(expire, flags)).await
}

#[asynchronous]
#[allow(clippy::unwrap_used)]
pub async fn sleep(duration: Duration) -> Result<()> {
	timeout(duration.as_nanos().try_into().unwrap(), BitFlags::default()).await
}
