use std::{
	ffi::CStr,
	io::Result,
	os::fd::{BorrowedFd, OwnedFd}
};

use xx_core::{
	os::socket::{MessageHeader, Shutdown},
	pointer::{ConstPtr, MutPtr},
	task::RequestPtr
};

use self::uring::IoUring;

mod uring;

pub trait EngineImpl {
	fn has_work(&self) -> bool;
	fn work(&mut self, timeout: u64) -> Result<()>;

	unsafe fn cancel(&mut self, request: RequestPtr<()>) -> Result<()>;

	unsafe fn open(
		&mut self, path: &CStr, flags: u32, mode: u32, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn close(
		&mut self, fd: OwnedFd, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn read(
		&mut self, fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn write(
		&mut self, fd: BorrowedFd<'_>, buf: &[u8], offset: i64, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn socket(
		&mut self, domain: u32, socket_type: u32, protocol: u32, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn accept(
		&mut self, socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn connect(
		&mut self, socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn recv(
		&mut self, socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn recvmsg(
		&mut self, socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn send(
		&mut self, socket: BorrowedFd<'_>, buf: &[u8], flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn sendmsg(
		&mut self, socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn shutdown(
		&mut self, socket: BorrowedFd<'_>, how: Shutdown, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn bind(
		&mut self, socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;

	unsafe fn listen(
		&mut self, socket: BorrowedFd<'_>, backlog: i32, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>>;
}

pub struct Engine;

/// I/O Backend
///
/// Could be one of io_uring, epoll, kqueue, iocp, etc
impl Engine {
	pub fn new() -> Result<Box<dyn EngineImpl>> {
		Ok(Box::new(IoUring::new()?))
	}
}
