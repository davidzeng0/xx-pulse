use xx_core::{
	container_of, coroutines::*, error::*, fiber::*, opt::hint::*, pointer::*,
	task::block_on as sync_block_on
};

use crate::{driver::Driver, *};

static mut POOL: Pool = Pool::new();

pub(super) struct RuntimeContext {
	executor: Handle<Executor>,
	driver: Handle<Driver>,
	context: Context
}

impl RuntimeContext {
	fn new(executor: Handle<Executor>, driver: Handle<Driver>, worker: Handle<Worker>) -> Self {
		Self {
			executor,
			driver,
			context: Context::new::<Self>(worker)
		}
	}

	pub fn driver(&mut self) -> Handle<Driver> {
		self.driver
	}
}

impl Global for RuntimeContext {}

impl PerContextRuntime for RuntimeContext {
	fn context(&mut self) -> &mut Context {
		&mut self.context
	}

	fn from_context(context: &mut Context) -> &mut Self {
		unsafe { container_of!(context, RuntimeContext, context) }
	}

	fn new_from_worker(&mut self, worker: Handle<Worker>) -> Self {
		RuntimeContext::new(self.executor(), self.driver, worker)
	}

	fn executor(&mut self) -> Handle<Executor> {
		self.executor
	}
}

pub struct Runtime {
	driver: Driver,
	executor: Executor
}

impl Runtime {
	pub fn new() -> Result<Boxed<Self>> {
		let runtime = Self { driver: Driver::new()?, executor: Executor::new() };

		Ok(Boxed::new(runtime))
	}

	pub fn block_on<T: Task>(&mut self, task: T) -> T::Output {
		let mut handle = Handle::from(self);
		let driver = (&mut handle.driver).into();
		let executor = (&mut handle.executor).into();

		let task = unsafe {
			spawn_sync(
				executor,
				|worker| RuntimeContext::new(executor, driver, worker),
				task
			)
		};

		let mut running = true;
		let running = MutPtr::from(&mut running);

		sync_block_on(
			|_| {
				let driver = &mut handle.driver;

				loop {
					let timeout = driver.run_timers();

					if unlikely(!*running) {
						break;
					}

					driver.park(timeout).unwrap();

					if unlikely(!*running) {
						break;
					}
				}
			},
			|| {
				*(running.clone()) = false;
			},
			task
		)
	}
}

impl Drop for Runtime {
	fn drop(&mut self) {
		self.driver.exit().unwrap();
	}
}

impl Global for Runtime {
	unsafe fn pinned(&mut self) {
		self.driver.pinned();
		self.executor.pinned();
		self.executor.set_pool((&mut POOL).into());
	}
}
