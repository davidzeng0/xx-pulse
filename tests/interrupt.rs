use std::time::Duration;

use xx_core::coroutines::{interrupt_guard, take_interrupt};
use xx_pulse::*;

#[async_fn]
async fn uninterruptible() {
	let guard = interrupt_guard().await;

	let _ = sleep(Duration::from_secs(1)).await;

	for _ in 0..200 {
		interrupt_guard().await;
	}

	let interrupted = is_interrupted().await;

	assert!(!interrupted);

	drop(guard);

	let interrupted = is_interrupted().await;

	assert!(interrupted);
}

#[async_fn]
async fn interruptible() {
	sleep(Duration::from_secs(1)).await.unwrap_err();

	let interrupted = is_interrupted().await;

	assert!(interrupted);

	sleep(Duration::from_secs(1)).await.unwrap_err();

	take_interrupt().await;

	let interrupted = is_interrupted().await;

	assert!(!interrupted);

	sleep(Duration::from_secs(1)).await.unwrap();
}

#[main]
#[test]
async fn test_interrupt() {
	let task = spawn(uninterruptible()).await;

	task.async_cancel().unwrap_err();

	let mut task = spawn(interruptible()).await;

	unsafe {
		task.request_cancel().unwrap();
	}

	task.await;
}
