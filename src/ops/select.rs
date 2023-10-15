use std::{io::Result, marker::PhantomData};

use xx_core::{
	coroutines::{runtime::block_on, spawn, task::AsyncTask},
	pin_local_mut,
	pointer::{ConstPtr, MutPtr},
	task::{env::Global, sync_task, Cancel, Progress, Request, RequestPtr, Task},
	warn
};

use super::*;

pub enum Select<O1, O2> {
	First(O1),
	Second(O2)
}

struct SelectData<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel> {
	task_1: Option<T1>,
	req_1: Request<O1>,
	cancel_1: Option<C1>,

	task_2: Option<T2>,
	req_2: Request<O2>,
	cancel_2: Option<C2>,

	result: Option<Select<O1, O2>>,
	request: RequestPtr<Select<O1, O2>>,
	sync_done: bool,
	phantom: PhantomData<(C1, C2)>
}

impl<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel>
	SelectData<O1, O2, T1, C1, T2, C2>
{
	fn complete(&mut self, value: Select<O1, O2>) {
		if self.sync_done {
			self.sync_done = false;

			return;
		}

		/*
		 * Safety: cannot access `self` once a cancel or a complete is called,
		 * as it may be freed by the callee
		 */
		if self.result.is_none() {
			let is_first = match &value {
				Select::First(_) => true,
				Select::Second(_) => false
			};

			self.result = Some(value);

			let result = if is_first {
				unsafe { self.cancel_2.take().unwrap().run() }
			} else {
				unsafe { self.cancel_1.take().unwrap().run() }
			};

			if result.is_err() {
				warn!("Cancel returned an {:?}", result);
			}
		} else {
			Request::complete(self.request, self.result.take().unwrap());
		}
	}

	fn complete_1(_: RequestPtr<O1>, arg: *const (), value: O1) {
		let mut data: MutPtr<Self> = ConstPtr::from(arg).cast();

		data.complete(Select::First(value));
	}

	fn complete_2(_: RequestPtr<O2>, arg: *const (), value: O2) {
		let mut data: MutPtr<Self> = ConstPtr::from(arg).cast();

		data.complete(Select::Second(value));
	}

	fn new(task_1: T1, task_2: T2) -> Self {
		let null = ConstPtr::<()>::null().as_raw_ptr();

		unsafe {
			Self {
				task_1: Some(task_1),
				req_1: Request::new(null, Self::complete_1),
				cancel_1: None,
				task_2: Some(task_2),
				req_2: Request::new(null, Self::complete_2),
				cancel_2: None,
				result: None,
				request: ConstPtr::null(),
				sync_done: false,
				phantom: PhantomData
			}
		}
	}

	#[sync_task]
	fn select(&mut self) -> Select<O1, O2> {
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
				Progress::Pending(cancel) => self.cancel_1 = Some(cancel),
				Progress::Done(value) => return Progress::Done(Select::First(value))
			}

			match self.task_2.take().unwrap().run(ConstPtr::from(&self.req_2)) {
				Progress::Pending(cancel) => self.cancel_2 = Some(cancel),
				Progress::Done(value) => {
					self.result = Some(Select::Second(value));
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

impl<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel> Global
	for SelectData<O1, O2, T1, C1, T2, C2>
{
	unsafe fn pinned(&mut self) {
		let arg: MutPtr<Self> = self.into();

		self.req_1.set_arg(arg.as_raw_ptr());
		self.req_2.set_arg(arg.as_raw_ptr());
	}
}

#[async_fn]
pub async fn select_sync<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel>(
	task_1: T1, task_2: T2
) -> Select<O1, O2> {
	let data = SelectData::new(task_1, task_2);

	pin_local_mut!(data);
	block_on(data.select()).await
}

#[async_fn]
pub async fn select<O1, O2, T1: AsyncTask<Context, O1>, T2: AsyncTask<Context, O2>>(
	task_1: T1, task_2: T2
) -> Select<O1, O2> {
	let executor = internal_get_context().await.executor();
	let driver = internal_get_driver().await;

	let task_1 = spawn::spawn(executor, |worker| {
		(Context::new(executor, worker, driver), task_1)
	});

	let task_2 = spawn::spawn(executor, |worker| {
		(Context::new(executor, worker, driver), task_2)
	});

	select_sync(task_1, task_2).await
}
