use std::time::Duration;

use xx_core::coroutines::{interrupt_guard, is_interrupted, take_interrupt};
use xx_pulse::*;

#[asynchronous]
async fn uninterruptible() {
	unsafe {
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
}

#[asynchronous]
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

	unsafe { task.async_cancel() }.unwrap_err();

	let mut task = spawn(interruptible()).await;

	unsafe {
		task.request_cancel().unwrap();
	}

	task.await;
}
