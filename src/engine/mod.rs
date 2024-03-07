use std::{
	ffi::CStr,
	os::fd::{BorrowedFd, FromRawFd, IntoRawFd, OwnedFd}
};

use paste::paste;
use xx_core::{
	error::Result,
	future::*,
	os::{error::result_from_int, socket::*, stat::Statx, unistd::close},
	pointer::*
};

use self::uring::IoUring;

mod uring;

#[allow(dead_code)]
pub enum OperationKind {
	Async,
	SyncOffload,
	NonBlocking
}

/// An implementation for engine
///
/// All asynchronous operations are unsafe, as the user
/// must preserve all arguments in memory until the callback is called
///
/// The result must be interpreted correctly, in order
/// to prevent memory and/or file descriptor leaks
pub trait EngineImpl {
	fn has_work(&self) -> bool;
	fn work(&mut self, timeout: u64) -> Result<()>;

	unsafe fn cancel(&mut self, request: ReqPtr<()>) -> Result<()>;

	fn open_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn open(
		&mut self, _path: &CStr, _flags: u32, _mode: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn close_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn close(&mut self, _fd: OwnedFd, _request: ReqPtr<isize>) -> Option<isize> {
		panic!();
	}

	fn read_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn read(
		&mut self, _fd: BorrowedFd<'_>, _buf: &mut [u8], _offset: i64, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn write_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn write(
		&mut self, _fd: BorrowedFd<'_>, _buf: &[u8], _offset: i64, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn socket_kind(&self) -> OperationKind {
		OperationKind::NonBlocking
	}

	unsafe fn socket(
		&mut self, _domain: u32, _socket_type: u32, _protocol: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn accept_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn accept(
		&mut self, _socket: BorrowedFd<'_>, _addr: MutPtr<()>, _addrlen: &mut u32,
		_request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn connect_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn connect(
		&mut self, _socket: BorrowedFd<'_>, _addr: Ptr<()>, _addrlen: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn recv_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn recv(
		&mut self, _socket: BorrowedFd<'_>, _buf: &mut [u8], _flags: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn recvmsg_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn recvmsg(
		&mut self, _socket: BorrowedFd<'_>, _header: &mut MessageHeaderMut<'_>, _flags: u32,
		_request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn send_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn send(
		&mut self, _socket: BorrowedFd<'_>, _buf: &[u8], _flags: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn sendmsg_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn sendmsg(
		&mut self, _socket: BorrowedFd<'_>, _header: &MessageHeader<'_>, _flags: u32,
		_request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn shutdown_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn shutdown(
		&mut self, _socket: BorrowedFd<'_>, _how: Shutdown, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn bind_kind(&self) -> OperationKind {
		OperationKind::NonBlocking
	}

	unsafe fn bind(
		&mut self, _socket: BorrowedFd<'_>, _addr: Ptr<()>, _addrlen: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn listen_kind(&self) -> OperationKind {
		OperationKind::NonBlocking
	}

	unsafe fn listen(
		&mut self, _socket: BorrowedFd<'_>, _backlog: i32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn fsync_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn fsync(&mut self, _file: BorrowedFd<'_>, _request: ReqPtr<isize>) -> Option<isize> {
		panic!();
	}

	fn statx_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn statx(
		&mut self, _dirfd: Option<BorrowedFd<'_>>, _path: &CStr, _flags: u32, _mask: u32,
		_statx: &mut Statx, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	fn poll_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn poll(
		&mut self, _fd: BorrowedFd<'_>, _mask: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}
}

pub struct SyncEngine {}

impl SyncEngine {
	fn sync_result(result: Result<isize>) -> isize {
		match result {
			Ok(num) => num,
			Err(err) => -(err.os_error().unwrap() as isize)
		}
	}
}

#[allow(unused_variables)]
impl EngineImpl for SyncEngine {
	fn has_work(&self) -> bool {
		false
	}

	fn work(&mut self, _: u64) -> Result<()> {
		// TODO: sleep?
		Ok(())
	}

	unsafe fn cancel(&mut self, _: ReqPtr<()>) -> Result<()> {
		/* nothing to cancel */
		Ok(())
	}

	unsafe fn open(
		&mut self, path: &CStr, flags: u32, mode: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn close(&mut self, fd: OwnedFd, _: ReqPtr<isize>) -> Option<isize> {
		Some(Self::sync_result(close(fd).map(|_| 0)))
	}

	unsafe fn read(
		&mut self, fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn write(
		&mut self, fd: BorrowedFd<'_>, buf: &[u8], offset: i64, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn socket(
		&mut self, domain: u32, socket_type: u32, protocol: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		Some(Self::sync_result(
			socket(domain, socket_type, protocol).map(|fd| fd.into_raw_fd() as isize)
		))
	}

	unsafe fn accept(
		&mut self, socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn connect(
		&mut self, socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn recv(
		&mut self, socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn recvmsg(
		&mut self, socket: BorrowedFd<'_>, header: &mut MessageHeaderMut<'_>, flags: u32,
		_: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn send(
		&mut self, socket: BorrowedFd<'_>, buf: &[u8], flags: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn sendmsg(
		&mut self, socket: BorrowedFd<'_>, header: &MessageHeader<'_>, flags: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn shutdown(
		&mut self, socket: BorrowedFd<'_>, how: Shutdown, _: ReqPtr<isize>
	) -> Option<isize> {
		Some(Self::sync_result(shutdown(socket, how).map(|_| 0)))
	}

	unsafe fn bind(
		&mut self, socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		Some(Self::sync_result(
			bind_raw(socket, addr, addrlen).map(|_| 0)
		))
	}

	unsafe fn listen(
		&mut self, socket: BorrowedFd<'_>, backlog: i32, _: ReqPtr<isize>
	) -> Option<isize> {
		Some(Self::sync_result(listen(socket, backlog).map(|_| 0)))
	}

	unsafe fn fsync(&mut self, file: BorrowedFd<'_>, _: ReqPtr<isize>) -> Option<isize> {
		panic!();
	}

	unsafe fn statx(
		&mut self, dirfd: Option<BorrowedFd<'_>>, path: &CStr, flags: u32, mask: u32,
		statx: &mut Statx, _: ReqPtr<isize>
	) -> Option<isize> {
		panic!();
	}

	unsafe fn poll(&mut self, fd: BorrowedFd<'_>, mask: u32, _: ReqPtr<isize>) -> Option<isize> {
		panic!();
	}
}

/// I/O Backend
///
/// Could be one of io_uring, epoll, kqueue, iocp, etc
pub struct Engine {
	#[cfg(target_os = "linux")]
	inner: IoUring
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

impl FromEngineResult for Result<u32> {
	fn from(val: isize) -> Self {
		result_from_int(val).map(|result| result as u32)
	}
}

impl FromEngineResult for Result<OwnedFd> {
	fn from(val: isize) -> Self {
		result_from_int(val).map(|raw_fd| unsafe { OwnedFd::from_raw_fd(raw_fd as i32) })
	}
}

macro_rules! engine_task {
	($func: ident ($($arg: ident: $type: ty),*) -> $return_type: ty) => {
		#[future]
		#[inline(always)]
        pub unsafe fn $func(&mut self, $($arg: $type),*) -> isize {
			#[cancel]
			fn cancel(self: &mut Self) -> Result<()> {
				unsafe { self.inner.cancel(request.cast()) }
			}

			match self.inner.$func($($arg),*, request) {
				None => Progress::Pending(cancel(self, request)),
				Some(result) => Progress::Done(result),
			}
        }

		paste! {
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

	#[inline(always)]
	pub fn has_work(&self) -> bool {
		self.inner.has_work()
	}

	#[inline(always)]
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

	engine_task!(connect(socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32) -> Result<()>);

	engine_task!(recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32) -> Result<usize>);

	engine_task!(recvmsg(socket: BorrowedFd<'_>, header: &mut MessageHeaderMut<'_>, flags: u32) -> Result<usize>);

	engine_task!(send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32) -> Result<usize>);

	engine_task!(sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader<'_>, flags: u32) -> Result<usize>);

	engine_task!(shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()>);

	engine_task!(bind(socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32) -> Result<()>);

	engine_task!(listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()>);

	engine_task!(fsync(file: BorrowedFd<'_>) -> Result<()>);

	engine_task!(statx(dirfd: Option<BorrowedFd<'_>>, path: &CStr, flags: u32, mask: u32, statx: &mut Statx) -> Result<()>);

	engine_task!(poll(fd: BorrowedFd<'_>, mask: u32) -> Result<u32>);
}
