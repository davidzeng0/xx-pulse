use std::{
	ffi::CString,
	io::Result,
	marker::PhantomData,
	mem::size_of,
	os::{
		fd::{BorrowedFd, FromRawFd, OwnedFd},
		unix::prelude::OsStrExt
	},
	path::Path
};

use enumflags2::BitFlags;
use xx_core::{
	coroutines::{
		spawn,
		task::{AsyncTask, BlockOn}
	},
	os::socket::{MessageHeader, Shutdown},
	pin_local_mut,
	pointer::{ConstPtr, MutPtr},
	task::{
		env::{Global, Handle},
		sync_task, Cancel, Progress, Request, RequestPtr, Task
	},
	warn
};

use crate::{
	async_runtime::*,
	driver::{Driver, TimeoutFlag}
};

#[async_fn]
async fn internal_get_context() -> Handle<Context> {
	__xx_async_internal_context
}

#[async_fn]
async fn internal_get_driver() -> Handle<Driver> {
	internal_get_context().await.driver()
}

#[async_fn]
pub async fn timeout(expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
	BlockOn::new(internal_get_driver().await.timeout(expire, flags)).await
}

#[async_fn]
pub async fn open(path: &Path, flags: u32, mode: u32) -> Result<OwnedFd> {
	let path = CString::new(path.as_os_str().as_bytes()).expect("Invalid path");
	let fd = BlockOn::new(internal_get_driver().await.open(path.as_ref(), flags, mode)).await?;

	Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

#[async_fn]
pub async fn close(fd: OwnedFd) -> Result<()> {
	BlockOn::new(internal_get_driver().await.close(fd)).await?;

	Ok(())
}

#[async_fn]
pub async fn read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64) -> Result<usize> {
	BlockOn::new(internal_get_driver().await.read(fd, buf, offset)).await
}

#[async_fn]
pub async fn write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64) -> Result<usize> {
	BlockOn::new(internal_get_driver().await.write(fd, buf, offset)).await
}

#[async_fn]
pub async fn socket(domain: u32, socket_type: u32, protocol: u32) -> Result<OwnedFd> {
	let fd = BlockOn::new(
		internal_get_driver()
			.await
			.socket(domain, socket_type, protocol)
	)
	.await?;

	Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

#[async_fn]
pub async fn accept_raw(
	socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32
) -> Result<OwnedFd> {
	let fd = BlockOn::new(internal_get_driver().await.accept(socket, addr, addrlen)).await?;

	Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

#[async_fn]
pub async fn accept<A>(socket: BorrowedFd<'_>, addr: &mut A) -> Result<(OwnedFd, u32)> {
	let mut addrlen = size_of::<A>() as u32;
	let fd = accept_raw(socket, MutPtr::from(addr).cast(), &mut addrlen).await?;

	Ok((fd, addrlen))
}

#[async_fn]
pub async fn connect_raw(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()> {
	BlockOn::new(internal_get_driver().await.connect(socket, addr, addrlen)).await?;

	Ok(())
}

#[async_fn]
pub async fn connect<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	connect_raw(socket, ConstPtr::from(addr).cast(), size_of::<A>() as u32).await
}

#[async_fn]
pub async fn recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32) -> Result<usize> {
	BlockOn::new(internal_get_driver().await.recv(socket, buf, flags)).await
}

#[async_fn]
pub async fn recvmsg(
	socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32
) -> Result<usize> {
	BlockOn::new(internal_get_driver().await.recvmsg(socket, header, flags)).await
}

#[async_fn]
pub async fn send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32) -> Result<usize> {
	BlockOn::new(internal_get_driver().await.send(socket, buf, flags)).await
}

#[async_fn]
pub async fn sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32) -> Result<usize> {
	BlockOn::new(internal_get_driver().await.sendmsg(socket, header, flags)).await
}

#[async_fn]
pub async fn shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()> {
	BlockOn::new(internal_get_driver().await.shutdown(socket, how)).await?;

	Ok(())
}

#[async_fn]
pub async fn bind_raw(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()> {
	BlockOn::new(internal_get_driver().await.bind(socket, addr, addrlen)).await?;

	Ok(())
}

#[async_fn]
pub async fn bind<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	bind_raw(socket, ConstPtr::from(addr).cast(), size_of::<A>() as u32).await
}

#[async_fn]
pub async fn listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()> {
	BlockOn::new(internal_get_driver().await.listen(socket, backlog)).await?;

	Ok(())
}

fn spawn_complete<Output>(ptr: RequestPtr<Output>, _: *const (), _: Output) {
	unsafe {
		let ptr: MutPtr<Request<Output>> = ptr.cast();

		drop(Box::from_raw(ptr.as_ptr_mut()));
	}
}

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
