#![allow(unreachable_pub, clippy::module_name_repetitions)]

use std::os::fd::{IntoRawFd, OwnedFd, RawFd};

use paste::paste;
use xx_core::{
	error::*,
	future::*,
	os::{
		socket::{raw::MsgHdr, *},
		stat::Statx,
		syscall::SyscallResult,
		unistd::close_raw
	},
	pointer::*
};

mod uring;
use uring::IoUring;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
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
#[allow(dead_code)]
pub trait EngineImpl {
	fn has_work(&self) -> bool;
	fn work(&self, timeout: u64) -> Result<()>;

	unsafe fn cancel(&self, request: ReqPtr<()>) -> Result<()>;

	fn open_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn open(
		&self, _path: Ptr<()>, _flags: u32, _mode: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn close_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn close(&self, _fd: RawFd, _request: ReqPtr<isize>) -> Option<isize> {
		unimplemented!();
	}

	fn read_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn read(
		&self, _fd: RawFd, _buf: MutPtr<()>, _len: usize, _offset: i64, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn write_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn write(
		&self, _fd: RawFd, _buf: Ptr<()>, _len: usize, _offset: i64, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn socket_kind(&self) -> OperationKind {
		OperationKind::NonBlocking
	}

	unsafe fn socket(
		&self, _domain: u32, _socket_type: u32, _protocol: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn accept_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn accept(
		&self, _socket: RawFd, _addr: MutPtr<()>, _addrlen: MutPtr<i32>, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn connect_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn connect(
		&self, _socket: RawFd, _addr: Ptr<()>, _addrlen: i32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn recv_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn recv(
		&self, _socket: RawFd, _buf: MutPtr<()>, _len: usize, _flags: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn recvmsg_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn recvmsg(
		&self, _socket: RawFd, _header: MutPtr<MsgHdr>, _flags: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn send_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn send(
		&self, _socket: RawFd, _buf: Ptr<()>, _len: usize, _flags: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn sendmsg_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn sendmsg(
		&self, _socket: RawFd, _header: Ptr<MsgHdr>, _flags: u32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn shutdown_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn shutdown(&self, _socket: RawFd, _how: u32, _request: ReqPtr<isize>) -> Option<isize> {
		unimplemented!();
	}

	fn bind_kind(&self) -> OperationKind {
		OperationKind::NonBlocking
	}

	unsafe fn bind(
		&self, _socket: RawFd, _addr: Ptr<()>, _addrlen: i32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn listen_kind(&self) -> OperationKind {
		OperationKind::NonBlocking
	}

	unsafe fn listen(
		&self, _socket: RawFd, _backlog: i32, _request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn fsync_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn fsync(&self, _file: RawFd, _request: ReqPtr<isize>) -> Option<isize> {
		unimplemented!();
	}

	fn statx_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn statx(
		&self, _dirfd: RawFd, _path: Ptr<()>, _flags: u32, _mask: u32, _statx: MutPtr<Statx>,
		_request: ReqPtr<isize>
	) -> Option<isize> {
		unimplemented!();
	}

	fn poll_kind(&self) -> OperationKind {
		OperationKind::SyncOffload
	}

	unsafe fn poll(&self, _fd: RawFd, _mask: u32, _request: ReqPtr<isize>) -> Option<isize> {
		unimplemented!();
	}
}

pub struct SyncEngine {}

impl SyncEngine {
	fn sync_result(result: Result<isize>) -> isize {
		#[allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]
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

	fn work(&self, _: u64) -> Result<()> {
		// TODO: sleep?
		Ok(())
	}

	unsafe fn cancel(&self, _: ReqPtr<()>) -> Result<()> {
		/* nothing to cancel */
		Ok(())
	}

	unsafe fn close(&self, fd: RawFd, _: ReqPtr<isize>) -> Option<isize> {
		/* Safety: guaranteed by caller */
		let result = unsafe { close_raw(fd) };

		Some(Self::sync_result(result.map(|()| 0)))
	}

	unsafe fn socket(
		&self, domain: u32, socket_type: u32, protocol: u32, _: ReqPtr<isize>
	) -> Option<isize> {
		let result = socket(domain, socket_type, protocol);

		Some(Self::sync_result(
			result.map(|fd| fd.into_raw_fd() as isize)
		))
	}

	unsafe fn shutdown(&self, socket: RawFd, how: u32, _: ReqPtr<isize>) -> Option<isize> {
		/* Safety: guaranteed by caller */
		let result = unsafe { shutdown_raw(socket, how) };

		Some(Self::sync_result(result.map(|()| 0)))
	}

	unsafe fn bind(
		&self, socket: RawFd, addr: Ptr<()>, addrlen: i32, _: ReqPtr<isize>
	) -> Option<isize> {
		/* Safety: guaranteed by caller */
		let result = unsafe { bind_raw(socket, addr, addrlen) };

		Some(Self::sync_result(result.map(|()| 0)))
	}

	unsafe fn listen(&self, socket: RawFd, backlog: i32, _: ReqPtr<isize>) -> Option<isize> {
		/* Safety: guaranteed by caller */
		let result = unsafe { listen_raw(socket, backlog) };

		Some(Self::sync_result(result.map(|()| 0)))
	}
}

/// I/O Backend
///
/// Could be one of io_uring, epoll, kqueue, iocp, etc
pub struct Engine {
	#[cfg(target_os = "linux")]
	inner: IoUring
}

macro_rules! engine_task {
	($func: ident ($($arg: ident: $type: ty),*) -> $return_type: ty) => {
		#[future]
		#[inline(always)]
		pub unsafe fn $func(&self, $($arg: $type),*) -> isize {
			#[cancel]
			fn cancel(&self) -> Result<()> {
				/* Safety: caller must enfore Future's contract */
				unsafe { self.inner.cancel(request.cast()) }
			}

			/* Safety: caller must uphold Future's contract */
			match unsafe { self.inner.$func($($arg,)* request) } {
				None => Progress::Pending(cancel(self, request)),
				Some(result) => Progress::Done(result),
			}
		}

		paste! {
			pub fn [<result_for_ $func>](val: isize) -> $return_type {
				SyscallResult(val).into()
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
	pub fn work(&self, timeout: u64) -> Result<()> {
		self.inner.work(timeout)
	}
}

impl Engine {
	engine_task!(open(path: Ptr<()>, flags: u32, mode: u32) -> Result<OwnedFd>);

	engine_task!(close(fd: RawFd) -> Result<()>);

	engine_task!(read(fd: RawFd, buf: MutPtr<()>, len: usize, offset: i64) -> Result<usize>);

	engine_task!(write(fd: RawFd, buf: Ptr<()>, len: usize, offset: i64) -> Result<usize>);

	engine_task!(socket(domain: u32, sockettype: u32, protocol: u32) -> Result<OwnedFd>);

	engine_task!(accept(socket: RawFd, addr: MutPtr<()>, addrlen: MutPtr<i32>) -> Result<OwnedFd>);

	engine_task!(connect(socket: RawFd, addr: Ptr<()>, addrlen: i32) -> Result<()>);

	engine_task!(recv(socket: RawFd, buf: MutPtr<()>, len: usize, flags: u32) -> Result<usize>);

	engine_task!(recvmsg(socket: RawFd, header: MutPtr<MsgHdr>, flags: u32) -> Result<usize>);

	engine_task!(send(socket: RawFd, buf: Ptr<()>, len: usize, flags: u32) -> Result<usize>);

	engine_task!(sendmsg(socket: RawFd, header: Ptr<MsgHdr>, flags: u32) -> Result<usize>);

	engine_task!(shutdown(socket: RawFd, how: u32) -> Result<()>);

	engine_task!(bind(socket: RawFd, addr: Ptr<()>, addrlen: i32) -> Result<()>);

	engine_task!(listen(socket: RawFd, backlog: i32) -> Result<()>);

	engine_task!(fsync(file: RawFd) -> Result<()>);

	engine_task!(statx(dirfd: RawFd, path: Ptr<()>, flags: u32, mask: u32, statx: MutPtr<Statx>) -> Result<()>);

	engine_task!(poll(fd: RawFd, mask: u32) -> Result<u32>);
}
