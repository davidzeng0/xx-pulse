#![allow(unreachable_pub)]

use std::cell::Cell;

use xx_core::{
	container::zero_alloc::linked_list::*, coroutines::get_context, debug, fiber::*,
	macros::container_of, opt::hint::*, pointer::*, runtime::join
};

use super::*;

pub struct Pulse {
	pub(crate) context: Context,
	pub(crate) driver: Ptr<Driver>,
	pub(crate) executor: Ptr<Executor>,
	pub(crate) workers: Ptr<LinkedList>
}

impl Pulse {
	/// # Safety
	/// See `Context::run`
	/// the executor, driver, and worker must live for self
	unsafe fn new(driver: Ptr<Driver>, executor: Ptr<Executor>, workers: Ptr<LinkedList>) -> Self {
		Self {
			/* Safety: guaranteed by caller */
			context: unsafe { Context::new::<Self>() },
			driver,
			executor,
			workers
		}
	}
}

/* Safety: functions don't panic */
unsafe impl Environment for Pulse {
	fn context(&self) -> &Context {
		&self.context
	}

	fn context_mut(&mut self) -> &mut Context {
		&mut self.context
	}

	unsafe fn from_context(context: &Context) -> &Self {
		let context = container_of!(ptr!(context), Self => context);

		/* Safety: guaranteed by caller */
		unsafe { context.as_ref() }
	}

	unsafe fn clone(&self) -> Self {
		/* Safety: guaranteed by caller */
		unsafe { Self::new(self.driver, self.executor, self.workers) }
	}

	fn executor(&self) -> Ptr<Executor> {
		self.executor
	}
}

pub struct PulseWorker {
	pub(crate) context: Ptr<Context>,
	pub(crate) node: Node
}

impl PulseWorker {
	/// # Safety
	/// must append to list before dropping
	#[asynchronous]
	pub async unsafe fn new() -> Self {
		Self { context: get_context().await, node: Node::new() }
	}
}

impl Drop for PulseWorker {
	fn drop(&mut self) {
		/* Safety: guaranteed by caller */
		unsafe { self.node.unlink_unchecked() };
	}
}

pub struct Runtime {
	driver: Driver,
	executor: Executor,
	workers: LinkedList,
	pool: Pool
}

impl Runtime {
	pub fn new() -> Result<Pinned<Box<Self>>> {
		let runtime = Self {
			driver: Driver::new()?,
			#[allow(clippy::multiple_unsafe_ops_per_block)]
			/* Safety: pool is valid */
			executor: Executor::new(),
			workers: LinkedList::new(),
			pool: Pool::new()
		};

		Ok(runtime.pin_box())
	}

	pub fn block_on<T>(&self, task: T) -> T::Output
	where
		T: Task
	{
		/* Safety: the env lives until the task finishes */
		#[allow(clippy::multiple_unsafe_ops_per_block)]
		let task = unsafe {
			coroutines::spawn_task(
				Pulse::new(
					ptr!(&self.driver),
					ptr!(&self.executor),
					ptr!(&self.workers)
				),
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
	#[allow(clippy::multiple_unsafe_ops_per_block)]
	fn drop(&mut self) {
		loop {
			/* to prevent busy looping, move all our nodes to a new list */
			let mut list = LinkedList::new();
			let list = list.pin_local();

			/* Safety: our new list is pinned, and we clear out all nodes before
			 * returning
			 */
			unsafe { self.workers.move_elements(&list) };

			if list.empty() {
				break;
			}

			while let Some(node) = list.pop_front() {
				let worker = container_of!(node, PulseWorker => node);

				/* Safety: all nodes are wrapped in PulseWorker */
				let context = unsafe { ptr!(worker=>context) };

				/* Safety: the worker may not exit immediately, and it unlinks itself on drop */
				unsafe { self.workers.append(node.as_ref()) };

				/* Safety: signal the task to interrupt. when the worker exits, it unlinks
				 * itself */
				let result = unsafe { Context::interrupt(context) };

				if let Err(err) = &result {
					debug!("Cancel was not successful: {:?}", err);
				}
			}

			/* complete any pending i/o */
			self.driver.exit();
		}
	}
}

impl Pin for Runtime {
	#[allow(clippy::multiple_unsafe_ops_per_block)]
	unsafe fn pin(&mut self) {
		/* Safety: we are being pinned */
		unsafe {
			self.executor.pin();
			self.executor.set_pool(ptr!(&self.pool));
			self.workers.pin();
		}
	}
}
