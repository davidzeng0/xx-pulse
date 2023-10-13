pub(crate) use xx_async_runtime::Context;
pub use xx_core::coroutines::async_fn_typed as async_fn;

pub mod xx_async_runtime {
	use std::io::Result;

	use xx_core::{
		closure::Closure,
		coroutines::{env::AsyncContext, executor::Executor, task::AsyncTask, worker::Worker},
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

		unsafe { cancel.take().unwrap().run() }
	}

	pub struct Context {
		executor: Handle<Executor>,
		worker: Handle<Worker>,
		driver: Handle<Driver>,

		cancel: Option<Closure<*const (), (), Result<()>>>
	}

	impl Context {
		pub(crate) fn new(
			executor: Handle<Executor>, worker: Handle<Worker>, driver: Handle<Driver>
		) -> Context {
			Context { executor, worker, driver, cancel: None }
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
		fn run<T: AsyncTask<Context, Output>, Output>(&mut self, task: T) -> Output {
			task.run(self.into())
		}

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
			self.cancel.take().unwrap().call(())
		}
	}

	impl Global for Context {}
}
