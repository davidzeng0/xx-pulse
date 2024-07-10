use xx_core::os::time::{self, ClockId};

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

#[asynchronous]
pub async fn yield_now() {
	let _ = sleep(Duration::ZERO).await;
}

#[allow(clippy::missing_panics_doc, clippy::expect_used)]
#[must_use]
pub fn nanotime() -> u64 {
	time::nanotime(ClockId::Monotonic).expect("Failed to read the clock")
}
