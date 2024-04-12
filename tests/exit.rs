#![allow(warnings)]

use std::{
	rc::Rc,
	sync::atomic::{AtomicBool, Ordering}
};

use xx_core::{async_std::sync::Notify, coroutines::take_interrupt, pointer::Pinned};
use xx_pulse::*;

static EXITED: AtomicBool = AtomicBool::new(false);

#[asynchronous]
async fn block2(notify: Pinned<Rc<Notify>>) {
	let result = select(notify.notified(), notify.notified()).await;

	result.flatten().unwrap_err();

	take_interrupt().await;

	let result = join(notify.notified(), notify.notified()).await;

	result.flatten().unwrap_err();
}

#[asynchronous]
async fn block(notify: Pinned<Rc<Notify>>) {
	for _ in 0..100 {
		notify.notified().await.unwrap_err();

		take_interrupt().await;

		spawn(block2(notify.clone())).await;

		println!("run");
	}

	EXITED.store(true, Ordering::Relaxed);

	panic!("test this too");
}

#[test]
fn test_exit() {
	#[main]
	async fn spawn_it() {
		let notify = Notify::<()>::new();

		spawn(block(notify.clone())).await;
	}

	spawn_it();

	assert!(EXITED.load(Ordering::Relaxed));
}
