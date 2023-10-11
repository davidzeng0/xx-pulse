use std::{io::Result, marker::PhantomData};

use xx_core::{
	coroutines::{
		spawn,
		task::{AsyncTask, BlockOn}
	},
	pin_local_mut,
	pointer::{ConstPtr, MutPtr},
	task::{env::Global, sync_task, Cancel, Progress, Request, RequestPtr, Task},
	warn
};

use super::*;

struct SelectData<Output, T1: Task<Output, C1>, C1: Cancel, T2: Task<Output, C2>, C2: Cancel> {
	task_1: Option<T1>,
	req_1: Request<Output>,
	cancel_1: Option<C1>,

	task_2: Option<T2>,
	req_2: Request<Output>,
	cancel_2: Option<C2>,

	result: Option<Output>,
	request: RequestPtr<Output>,
	sync_done: bool,
	phantom: PhantomData<(C1, C2)>
}

impl<Output, T1: Task<Output, C1>, C1: Cancel, T2: Task<Output, C2>, C2: Cancel>
	SelectData<Output, T1, C1, T2, C2>
{
	fn do_select(request: RequestPtr<Output>, arg: *const (), value: Output) {
		let mut data: MutPtr<Self> = ConstPtr::from(arg).cast();

		if data.sync_done {
			data.sync_done = false;

			return;
		}

		/*
		 * Safety: cannot access `data` once a cancel or a complete is called,
		 * as it may be freed by the callee
		 */
		if data.result.is_none() {
			data.result = Some(value);

			let result = if request == ConstPtr::from(&data.req_1) {
				unsafe { data.cancel_2.take().unwrap().run() }
			} else {
				unsafe { data.cancel_1.take().unwrap().run() }
			};

			if result.is_err() {
				warn!("Cancel returned an {:?}", result);
			}
		} else {
			Request::complete(data.request, data.result.take().unwrap());
		}
	}

	fn new(task_1: T1, task_2: T2) -> Self {
		let null = ConstPtr::<()>::null().as_raw_ptr();

		unsafe {
			Self {
				task_1: Some(task_1),
				req_1: Request::new(null, Self::do_select),
				cancel_1: None,
				task_2: Some(task_2),
				req_2: Request::new(null, Self::do_select),
				cancel_2: None,
				result: None,
				request: ConstPtr::null(),
				sync_done: false,
				phantom: PhantomData
			}
		}
	}

	#[sync_task]
	fn select(&mut self) -> Output {
		fn cancel(self: &mut Self) -> Result<()> {
			let (cancel_1, cancel_2) = unsafe {
				(
					self.cancel_1.take().map(|cancel| cancel.run()),
					self.cancel_2.take().map(|cancel| cancel.run())
				)
			};

			if let Some(Err(result)) = cancel_1 {
				return Err(result);
			}

			if let Some(Err(result)) = cancel_2 {
				return Err(result);
			}

			Ok(())
		}

		unsafe {
			match self.task_1.take().unwrap().run(ConstPtr::from(&self.req_1)) {
				Progress::Done(value) => return Progress::Done(value),
				Progress::Pending(cancel) => self.cancel_1 = Some(cancel)
			}

			match self.task_2.take().unwrap().run(ConstPtr::from(&self.req_2)) {
				Progress::Pending(cancel) => self.cancel_2 = Some(cancel),
				Progress::Done(value) => {
					self.result = Some(value);
					self.sync_done = true;

					let result = self.cancel_1.take().unwrap().run();

					if result.is_err() {
						warn!("Cancel returned an {:?}", result);
					}

					if !self.sync_done {
						return Progress::Done(self.result.take().unwrap());
					}

					self.sync_done = false;
				}
			}

			self.request = request;

			return Progress::Pending(cancel(self, request));
		}
	}
}

impl<Output, T1: Task<Output, C1>, C1: Cancel, T2: Task<Output, C2>, C2: Cancel> Global
	for SelectData<Output, T1, C1, T2, C2>
{
	unsafe fn pinned(&mut self) {
		let arg: MutPtr<Self> = self.into();

		self.req_1.set_arg(arg.as_raw_ptr());
		self.req_2.set_arg(arg.as_raw_ptr());
	}
}

#[async_fn]
pub async fn select<Output, T1: AsyncTask<Context, Output>, T2: AsyncTask<Context, Output>>(
	task_1: T1, task_2: T2
) -> Output {
	let executor = internal_get_context().await.executor();
	let driver = internal_get_driver().await;

	let data = {
		let t1 = spawn::spawn(executor, |worker| {
			(Context::new(executor, worker, driver), task_1)
		});

		let t2 = spawn::spawn(executor, |worker| {
			(Context::new(executor, worker, driver), task_2)
		});

		SelectData::new(t1, t2)
	};

	pin_local_mut!(data);

	BlockOn::new(data.select()).await
}
