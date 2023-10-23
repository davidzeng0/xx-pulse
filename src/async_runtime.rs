pub(crate) use xx_async_runtime::Context;
pub use xx_core::coroutines::async_fn_typed as async_fn;

pub mod xx_async_runtime {
	use xx_core::{
		closure::Closure,
		coroutines::{env::AsyncContext, executor::Executor, task::AsyncTask, worker::Worker},
		error::{Error, ErrorKind, Result},
		pointer::{ConstPtr, MutPtr},
		task::{
			block_on::block_on,
			env::{Global, Handle},
			Cancel, Task
		}
	};

	use crate::driver::Driver;

	fn run_cancel<C: Cancel>(arg: *const (), _: ()) -> Result<()> {
		let mut cancel: MutPtr<Option<C>> = ConstPtr::from(arg).cast();

		unsafe { cancel.take().unwrap_unchecked().run() }
	}

	pub struct Context {
		executor: Handle<Executor>,
		worker: Handle<Worker>,
		driver: Handle<Driver>,

		cancel: Option<Closure<*const (), (), Result<()>>>,

		guards: u32,
		interrupted: bool,
		pending_interrupt: bool
	}

	impl Context {
		pub(crate) fn new(
			executor: Handle<Executor>, worker: Handle<Worker>, driver: Handle<Driver>
		) -> Context {
			Context {
				executor,
				worker,
				driver,
				cancel: None,
				guards: 0,
				interrupted: false,
				pending_interrupt: false
			}
		}

		pub(crate) fn executor(&mut self) -> Handle<Executor> {
			self.executor
		}

		#[inline(always)]
		pub(crate) fn driver(&mut self) -> Handle<Driver> {
			self.driver
		}

		#[inline(always)]
		fn suspend(&mut self) {
			unsafe {
				self.worker.suspend();
			}
		}

		#[inline(always)]
		fn resume(&mut self) {
			unsafe {
				self.executor.resume(self.worker);
			}
		}
	}

	impl AsyncContext for Context {
		#[inline(always)]
		fn run<T: AsyncTask<Context, Output>, Output>(&mut self, task: T) -> Output {
			task.run(self.into())
		}

		#[inline(always)]
		fn block_on<T: Task<Output, C>, C: Cancel, Output>(&mut self, task: T) -> Output {
			let handle = Handle::from(self);

			block_on(
				|cancel| {
					let mut cancel = Some(cancel);

					handle.clone().cancel = Some(Closure::new(
						MutPtr::from(&mut cancel).as_raw_ptr(),
						run_cancel::<C>
					));

					handle.clone().suspend();
				},
				|| {
					handle.clone().resume();
				},
				task
			)
		}

		fn interrupt(&mut self) -> Result<()> {
			if self.guards > 0 {
				if !self.pending_interrupt {
					self.pending_interrupt = true;
				}

				Err(Error::new(ErrorKind::Other, "Interrupt is prevented"))
			} else {
				self.interrupted = true;
				self.cancel
					.take()
					.expect("Task interrupted while active")
					.call(())
			}
		}

		fn interrupted(&self) -> bool {
			self.interrupted
		}

		fn clear_interrupt(&mut self) {
			self.interrupted = false;
			self.pending_interrupt = false;
		}

		fn interrupt_guard(&mut self, count: i32) {
			self.guards = self
				.guards
				.checked_add_signed(count)
				.expect("Interrupt guards count overflowed");
			if self.guards == 0 && self.pending_interrupt {
				self.interrupted = true;
				self.pending_interrupt = false;
			}
		}
	}

	impl Global for Context {}
}
