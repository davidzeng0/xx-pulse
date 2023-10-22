use xx_core::{
	coroutines::{runtime::block_on, spawn as spawn_worker, task::AsyncTask},
	error::Result,
	pointer::*,
	task::*
};

use super::*;

struct Spawn<Output> {
	request: Request<Output>,
	cancel: Option<CancelClosure<(Handle<Context>, RequestPtr<Output>)>>,
	waiter: RequestPtr<Output>,
	output: Option<Output>,
	refs: u32
}

impl<Output> Spawn<Output> {
	fn dec_ref(&mut self) {
		self.refs -= 1;

		if self.refs == 0 {
			let this = MutPtr::from(self);
			let this = unsafe { Box::from_raw(this.as_ptr_mut()) };

			drop(this);
		}
	}

	fn inc_ref(&mut self) {
		self.refs += 1;
	}

	fn spawn_complete(_: RequestPtr<Output>, arg: *const (), output: Output) {
		let mut this: MutPtr<Spawn<Output>> = ConstPtr::from(arg).cast();

		if this.waiter.is_null() {
			this.output = Some(output);
		} else {
			Request::complete(this.waiter, output);
		}

		this.dec_ref();
	}

	pub fn new() -> Self {
		unsafe {
			Self {
				request: Request::new(ConstPtr::<()>::null().as_ptr(), Self::spawn_complete),
				cancel: None,
				waiter: RequestPtr::null(),
				output: None,
				refs: 0
			}
		}
	}

	#[async_fn]
	pub async fn run<Task: AsyncTask<Context, Output>>(task: Task) -> JoinHandle<Output> {
		let mut this = Box::new(Spawn::new());

		unsafe {
			this.as_mut().pinned();
		}

		let executor = internal_get_context().await.executor();
		let driver = internal_get_driver().await;

		let task = spawn_worker(executor, |worker| {
			(Context::new(executor, worker, driver), task)
		});

		unsafe {
			match task.run(ConstPtr::from(&this.request)) {
				Progress::Pending(cancel) => {
					this.cancel = Some(cancel);
					this.inc_ref();
				}

				Progress::Done(result) => {
					this.output = Some(result);
				}
			}
		}

		this.inc_ref();

		JoinHandle { task: MutPtr::from(Box::into_raw(this)) }
	}

	pub fn cancel(&mut self) -> Result<()> {
		unsafe { self.cancel.take().unwrap().run() }
	}
}

impl<Output> Global for Spawn<Output> {
	unsafe fn pinned(&mut self) {
		let arg: MutPtr<Self> = self.into();

		self.request.set_arg(arg.as_raw_ptr());
	}
}

struct JoinTask<Output> {
	task: MutPtr<Spawn<Output>>
}

struct JoinCancel<Output> {
	task: MutPtr<Spawn<Output>>
}

pub struct JoinHandle<Output> {
	task: MutPtr<Spawn<Output>>
}

unsafe impl<Output> Task<Output, JoinCancel<Output>> for JoinTask<Output> {
	unsafe fn run(mut self, request: RequestPtr<Output>) -> Progress<Output, JoinCancel<Output>> {
		self.task.waiter = request;

		Progress::Pending(JoinCancel { task: self.task })
	}
}

unsafe impl<Output> Cancel for JoinCancel<Output> {
	unsafe fn run(mut self) -> Result<()> {
		self.task.cancel()
	}
}

impl<Output> AsyncTask<Context, Output> for JoinHandle<Output> {
	fn run(mut self, context: Handle<Context>) -> Output {
		if let Some(output) = self.task.output.take() {
			return output;
		}

		block_on(JoinTask { task: self.task }).run(context)
	}
}

impl<Output> Drop for JoinHandle<Output> {
	fn drop(&mut self) {
		self.task.dec_ref();
	}
}

/// Spawn a new async task
///
/// Returns a join handle which may be used to get the result from the task
#[async_fn]
pub async fn spawn<Output, T: AsyncTask<Context, Output> + 'static>(task: T) -> JoinHandle<Output> {
	Spawn::run(task).await
}
