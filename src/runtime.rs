use std::cell::Cell;

use xx_core::{
	container_of, coroutines::*, error::*, fiber::*, future::block_on::block_on as sync_block_on,
	opt::hint::*, pointer::*
};

use crate::driver::Driver;

static mut POOL: Pool = Pool::new();

pub(super) struct Pulse {
	executor: Ptr<Executor>,
	driver: Ptr<Driver>,
	context: Context
}

impl Pulse {
	fn new(executor: Ptr<Executor>, driver: Ptr<Driver>, worker: Ptr<Worker>) -> Self {
		Self {
			executor,
			driver,
			context: Context::new::<Self>(worker)
		}
	}

	pub fn driver(&self) -> Ptr<Driver> {
		self.driver
	}
}

impl Environment for Pulse {
	fn context(&self) -> &Context {
		&self.context
	}

	fn from_context(context: &Context) -> Ptr<Self> {
		unsafe { container_of!(Ptr::from(context), Pulse:context) }.cast_const()
	}

	unsafe fn clone(&self, worker: Ptr<Worker>) -> Self {
		Pulse::new(self.executor(), self.driver, worker)
	}

	fn executor(&self) -> Ptr<Executor> {
		self.executor
	}
}

pub struct Runtime {
	driver: Driver,
	executor: Executor
}

impl Runtime {
	pub fn new() -> Result<Pinned<Box<Self>>> {
		let runtime = Self {
			driver: Driver::new()?,
			executor: unsafe { Executor::new_with_pool(Ptr::from(&POOL)) }
		};

		Ok(runtime.pin_box())
	}

	pub fn block_on<T: Task>(&mut self, task: T) -> T::Output {
		let task = unsafe {
			let driver = Ptr::from(&self.driver);
			let executor = Ptr::from(&self.executor);

			spawn_future(
				executor,
				move |worker| Pulse::new(executor, driver, worker),
				task
			)
		};

		let running = Cell::new(true);
		let block = |_| loop {
			let timeout = self.driver.run_timers();

			if unlikely(!running.get()) {
				break;
			}

			self.driver.park(timeout).unwrap();

			if unlikely(!running.get()) {
				break;
			}
		};

		let resume = || {
			running.set(false);
		};

		unsafe { sync_block_on(block, resume, task) }
	}
}

impl Drop for Runtime {
	fn drop(&mut self) {
		self.driver.exit().unwrap();
	}
}

unsafe impl Pin for Runtime {
	unsafe fn pin(&mut self) {
		self.executor.pin();
	}
}
