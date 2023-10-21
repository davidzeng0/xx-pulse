use std::{
	ffi::CStr,
	os::fd::{BorrowedFd, FromRawFd, OwnedFd}
};

use xx_core::{
	error::Result,
	os::{
		error::result_from_int,
		socket::{MessageHeader, Shutdown},
		stat::Statx
	},
	pointer::{ConstPtr, MutPtr},
	task::{sync_task, Progress, RequestPtr}
};

use self::uring::IoUring;

mod uring;

pub trait EngineImpl {
	fn has_work(&self) -> bool;
	fn work(&mut self, timeout: u64) -> Result<()>;

	unsafe fn cancel(&mut self, request: RequestPtr<()>) -> Result<()>;

	unsafe fn open(
		&mut self, path: &CStr, flags: u32, mode: u32, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn close(&mut self, fd: OwnedFd, request: RequestPtr<isize>) -> Option<isize>;

	unsafe fn read(
		&mut self, fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn write(
		&mut self, fd: BorrowedFd<'_>, buf: &[u8], offset: i64, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn socket(
		&mut self, domain: u32, socket_type: u32, protocol: u32, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn accept(
		&mut self, socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32,
		request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn connect(
		&mut self, socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32,
		request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn recv(
		&mut self, socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn recvmsg(
		&mut self, socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32,
		request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn send(
		&mut self, socket: BorrowedFd<'_>, buf: &[u8], flags: u32, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn sendmsg(
		&mut self, socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32,
		request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn shutdown(
		&mut self, socket: BorrowedFd<'_>, how: Shutdown, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn bind(
		&mut self, socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32,
		request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn listen(
		&mut self, socket: BorrowedFd<'_>, backlog: i32, request: RequestPtr<isize>
	) -> Option<isize>;

	unsafe fn fsync(&mut self, file: BorrowedFd<'_>, request: RequestPtr<isize>) -> Option<isize>;

	unsafe fn statx(
		&mut self, path: &CStr, flags: u32, mask: u32, statx: &mut Statx,
		request: RequestPtr<isize>
	) -> Option<isize>;
}

/// I/O Backend
///
/// Could be one of io_uring, epoll, kqueue, iocp, etc
pub struct Engine {
	#[cfg(target_os = "linux")]
	inner: IoUring<'static>
}

trait FromEngineResult {
	fn from(val: isize) -> Self;
}

impl FromEngineResult for Result<()> {
	fn from(val: isize) -> Self {
		result_from_int(val).map(|_| ())
	}
}

impl FromEngineResult for Result<usize> {
	fn from(val: isize) -> Self {
		result_from_int(val).map(|result| result as usize)
	}
}

impl FromEngineResult for Result<OwnedFd> {
	fn from(val: isize) -> Self {
		result_from_int(val).map(|raw_fd| unsafe { OwnedFd::from_raw_fd(raw_fd as i32) })
	}
}

macro_rules! engine_task {
	($func: ident ($($arg: ident: $type: ty),*) -> $return_type: ty) => {
		#[sync_task]
		#[inline(always)]
        pub fn $func(&mut self, $($arg: $type),*) -> isize {
			fn cancel(self: &mut Self) -> Result<()> {
				unsafe { self.inner.cancel(request.cast()) }
			}

			match unsafe { self.inner.$func($($arg),*, request) } {
				None => Progress::Pending(cancel(self, request)),
				Some(result) => Progress::Done(result),
			}
        }

		paste::paste! {
			pub fn [<result_for_ $func>](val: isize) -> $return_type {
				FromEngineResult::from(val)
			}
		}
    }
}

impl Engine {
	pub fn new() -> Result<Self> {
		#[cfg(target_os = "linux")]
		let inner = IoUring::new()?;

		Ok(Self { inner })
	}

	pub fn has_work(&self) -> bool {
		self.inner.has_work()
	}

	pub fn work(&mut self, timeout: u64) -> Result<()> {
		self.inner.work(timeout)
	}
}

impl Engine {
	engine_task!(open(path: &CStr, flags: u32, mode: u32) -> Result<OwnedFd>);

	engine_task!(close(fd: OwnedFd) -> Result<()>);

	engine_task!(read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64) -> Result<usize>);

	engine_task!(write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64) -> Result<usize>);

	engine_task!(socket(domain: u32, socket_type: u32, protocol: u32) -> Result<OwnedFd>);

	engine_task!(accept(socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32) -> Result<OwnedFd>);

	engine_task!(connect(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()>);

	engine_task!(recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32) -> Result<usize>);

	engine_task!(recvmsg(socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32) -> Result<usize>);

	engine_task!(send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32) -> Result<usize>);

	engine_task!(sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32) -> Result<usize>);

	engine_task!(shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()>);

	engine_task!(bind(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()>);

	engine_task!(listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()>);

	engine_task!(fsync(file: BorrowedFd<'_>) -> Result<()>);

	engine_task!(statx(path: &CStr, flags: u32, mask: u32, statx: &mut Statx) -> Result<()>);
}
