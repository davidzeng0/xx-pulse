use xx_core::{
	coroutines::{executor::Executor, spawn::spawn, task::AsyncTask},
	error::Result,
	task::{
		block_on::block_on,
		env::{Boxed, Global, Handle}
	}
};

use crate::{async_runtime::Context, driver::Driver};

pub struct Runtime {
	driver: Driver,
	executor: Executor
}

impl Runtime {
	pub fn new() -> Result<Boxed<Self>> {
		let runtime = Self { driver: Driver::new()?, executor: Executor::new() };

		Ok(Boxed::new(runtime))
	}

	pub fn block_on<F: Fn() -> Task, Task: AsyncTask<Context, Output>, Output>(
		&mut self, entry: F
	) -> Output {
		let mut handle = Handle::from(self);
		let driver = (&mut handle.driver).into();
		let executor = (&mut handle.executor).into();

		let task = spawn(executor, |worker| {
			(Context::new(executor, worker, driver), entry())
		});

		block_on(
			|_| {
				handle.driver.run().unwrap();
			},
			|| {},
			task
		)
	}
}

impl Global for Runtime {
	unsafe fn pinned(&mut self) {
		self.driver.pinned();
		self.executor.pinned();
	}
}
