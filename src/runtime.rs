#![allow(unreachable_pub)]

use std::cell::Cell;

use xx_core::{fiber::*, macros::container_of, opt::hint::*, pointer::*, runtime::join};

use super::*;

static POOL: Pool = Pool::new();

pub struct Pulse {
	executor: Ptr<Executor>,
	driver: Ptr<Driver>,
	context: Context
}

impl Pulse {
	/// # Safety
	/// See `Context::run`
	/// the executor, driver, and worker must live for self
	unsafe fn new(executor: Ptr<Executor>, driver: Ptr<Driver>, worker: Ptr<Worker>) -> Self {
		Self {
			executor,
			driver,
			/* Safety: guaranteed by caller */
			context: unsafe { Context::new::<Self>(worker) }
		}
	}

	#[must_use]
	pub const fn driver(&self) -> Ptr<Driver> {
		self.driver
	}
}

/* Safety: functions don't panic */
unsafe impl Environment for Pulse {
	fn context(&self) -> &Context {
		&self.context
	}

	unsafe fn from_context(context: &Context) -> &Self {
		let context = container_of!(ptr!(context), Self:context);

		/* Safety: guaranteed by caller */
		unsafe { context.as_ref() }
	}

	unsafe fn clone(&self, worker: Ptr<Worker>) -> Self {
		/* Safety: guaranteed by caller */
		unsafe { Self::new(self.executor(), self.driver, worker) }
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
			#[allow(clippy::multiple_unsafe_ops_per_block)]
			/* Safety: pool is valid */
			executor: unsafe { Executor::new_with_pool(ptr!(&POOL)) }
		};

		Ok(runtime.pin_box())
	}

	pub fn block_on<T>(&mut self, task: T) -> T::Output
	where
		T: Task
	{
		/* Safety: the env lives until the task finishes */
		#[allow(clippy::multiple_unsafe_ops_per_block)]
		let task = unsafe {
			let driver = ptr!(&self.driver);
			let executor = ptr!(&self.executor);

			coroutines::spawn_task(
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

			self.driver.park(timeout);

			if unlikely(!running.get()) {
				break;
			}
		};

		let resume = || {
			running.set(false);
		};

		/* Safety: we are blocked until the future completes */
		join(unsafe { block_on(block, resume, task) })
	}
}

impl Drop for Runtime {
	fn drop(&mut self) {
		#[allow(clippy::unwrap_used)]
		self.driver.exit();
	}
}

impl Pin for Runtime {
	unsafe fn pin(&mut self) {
		/* Safety: we are being pinned */
		unsafe { self.executor.pin() };
	}
}
