use std::{marker::PhantomData, result};

use xx_core::{
	coroutines::{runtime::block_on, spawn, task::AsyncTask},
	error::Result,
	pin_local_mut,
	pointer::{ConstPtr, MutPtr},
	task::{env::Global, sync_task, Cancel, Progress, Request, RequestPtr, Task}
};

use super::*;

pub struct Join<O1, O2>(pub O1, pub O2);

impl<O1, O2, E> Join<result::Result<O1, E>, result::Result<O2, E>> {
	/// Flatten the `Join``, returning the first error it encounters
	pub fn flatten(self) -> result::Result<Join<O1, O2>, E> {
		Ok(Join(self.0?, self.1?))
	}
}

impl<O1, O2> Join<Option<O1>, Option<O2>> {
	/// Flatten the `Join``, returning none if there are any
	pub fn flatten(self) -> Option<Join<O1, O2>> {
		Some(Join(self.0?, self.1?))
	}
}

struct JoinData<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel> {
	task_1: Option<T1>,
	req_1: Request<O1>,
	cancel_1: Option<C1>,
	result_1: Option<O1>,

	task_2: Option<T2>,
	req_2: Request<O2>,
	cancel_2: Option<C2>,
	result_2: Option<O2>,

	request: RequestPtr<Join<O1, O2>>,
	phantom: PhantomData<(C1, C2)>
}

impl<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel>
	JoinData<O1, O2, T1, C1, T2, C2>
{
	fn complete_1(_: RequestPtr<O1>, arg: *const (), value: O1) {
		let mut data: MutPtr<Self> = ConstPtr::from(arg).cast();

		data.result_1 = Some(value);
		data.check_complete();
	}

	fn complete_2(_: RequestPtr<O2>, arg: *const (), value: O2) {
		let mut data: MutPtr<Self> = ConstPtr::from(arg).cast();

		data.result_2 = Some(value);
		data.check_complete();
	}

	fn new(task_1: T1, task_2: T2) -> Self {
		let null = ConstPtr::<()>::null().as_raw_ptr();

		unsafe {
			Self {
				task_1: Some(task_1),
				req_1: Request::new(null, Self::complete_1),
				cancel_1: None,
				result_1: None,

				task_2: Some(task_2),
				req_2: Request::new(null, Self::complete_2),
				cancel_2: None,
				result_2: None,

				request: ConstPtr::null(),
				phantom: PhantomData
			}
		}
	}

	fn check_complete(&mut self) {
		if self.result_1.is_none() || self.result_2.is_none() {
			return;
		}

		/*
		 * Safety: cannot access `self` once a cancel or a complete is called,
		 * as it may be freed by the callee
		 */
		Request::complete(
			self.request,
			Join(self.result_1.take().unwrap(), self.result_2.take().unwrap())
		);
	}

	#[sync_task]
	fn join(&mut self) -> Join<O1, O2> {
		fn cancel(self: &mut Self) -> Result<()> {
			let (cancel_1, cancel_2) = unsafe {
				(
					self.cancel_1.take().unwrap().run(),
					self.cancel_2.take().unwrap().run()
				)
			};

			if cancel_1.is_err() {
				return cancel_1;
			}

			if cancel_2.is_err() {
				return cancel_2;
			}

			Ok(())
		}

		unsafe {
			match self.task_1.take().unwrap().run(ConstPtr::from(&self.req_1)) {
				Progress::Pending(cancel) => self.cancel_1 = Some(cancel),
				Progress::Done(value) => self.result_1 = Some(value)
			}

			match self.task_2.take().unwrap().run(ConstPtr::from(&self.req_2)) {
				Progress::Pending(cancel) => self.cancel_2 = Some(cancel),
				Progress::Done(value) => {
					if self.result_1.is_some() {
						return Progress::Done(Join(self.result_1.take().unwrap(), value));
					}

					self.result_2 = Some(value);
				}
			}

			self.request = request;

			return Progress::Pending(cancel(self, request));
		}
	}
}

impl<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel> Global
	for JoinData<O1, O2, T1, C1, T2, C2>
{
	unsafe fn pinned(&mut self) {
		let arg: MutPtr<Self> = self.into();

		self.req_1.set_arg(arg.as_raw_ptr());
		self.req_2.set_arg(arg.as_raw_ptr());
	}
}

#[async_fn]
pub async fn join_sync<O1, O2, T1: Task<O1, C1>, C1: Cancel, T2: Task<O2, C2>, C2: Cancel>(
	task_1: T1, task_2: T2
) -> Join<O1, O2> {
	let data = JoinData::new(task_1, task_2);

	pin_local_mut!(data);
	block_on(data.join()).await
}

/// Joins two tasks A and B and waits
/// for both of them to finish, returning
/// both of their results
#[async_fn]
pub async fn join<O1, O2, T1: AsyncTask<Context, O1>, T2: AsyncTask<Context, O2>>(
	task_1: T1, task_2: T2
) -> Join<O1, O2> {
	let executor = internal_get_context().await.executor();
	let driver = internal_get_driver().await;

	let task_1 = spawn::spawn(executor, |worker| {
		(Context::new(executor, worker, driver), task_1)
	});

	let task_2 = spawn::spawn(executor, |worker| {
		(Context::new(executor, worker, driver), task_2)
	});

	join_sync(task_1, task_2).await
}
