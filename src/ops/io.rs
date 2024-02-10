use std::{
	ffi::{CStr, CString},
	mem::size_of,
	os::{
		fd::{BorrowedFd, OwnedFd},
		unix::prelude::OsStrExt
	},
	path::Path
};

use xx_core::{
	error::*,
	os::{
		inet::Address,
		socket::{MsgHdr, Shutdown},
		stat::Statx
	},
	pointer::*
};

use super::*;
use crate::engine::Engine;

macro_rules! async_engine_task {
	($force: literal, $func: ident ($($arg: ident: $type: ty),*) -> $return_type: ty) => {
		#[asynchronous]
		#[inline(always)]
		pub async fn $func($($arg: $type),*) -> $return_type {
			let driver = unsafe { internal_get_driver().await.as_ref() };

			if !$force {
				check_interrupt().await?;

				driver.check_exiting()?;
			}

			let result = block_on(unsafe { driver.$func($($arg),*) }).await;

			paste::paste! { Engine::[<result_for_ $func>](result) }
		}
	}
}

fn path_to_cstr(path: &Path) -> Result<CString> {
	CString::new(path.as_os_str().as_bytes()).map_err(|_| Core::InvalidCStr.new())
}

mod internal {
	use super::*;

	async_engine_task!(false, open(path: &CStr, flags: u32, mode: u32) -> Result<OwnedFd>);

	async_engine_task!(false, accept(
		socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32
	) -> Result<OwnedFd>);

	async_engine_task!(false, connect(socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32) -> Result<()>);

	async_engine_task!(false, bind(socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32) -> Result<()>);

	async_engine_task!(false, statx(path: &CStr, flags: u32, mask: u32, statx: &mut Statx) -> Result<()>);
}

#[asynchronous]
pub async fn open(path: &Path, flags: u32, mode: u32) -> Result<OwnedFd> {
	let path = path_to_cstr(path)?;

	internal::open(&path, flags, mode).await
}

async_engine_task!(true, close(fd: OwnedFd) -> Result<()>);
async_engine_task!(false, read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64) -> Result<usize>);
async_engine_task!(false, write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64) -> Result<usize>);
async_engine_task!(false, socket(domain: u32, socket_type: u32, protocol: u32) -> Result<OwnedFd>);

use internal::accept as accept_raw;

#[asynchronous]
pub async fn accept<A>(socket: BorrowedFd<'_>, addr: &mut A) -> Result<(OwnedFd, u32)> {
	let mut addrlen = size_of::<A>() as u32;
	let fd = accept_raw(socket, MutPtr::from(addr).as_unit(), &mut addrlen).await?;

	Ok((fd, addrlen))
}

use internal::connect as connect_raw;

#[asynchronous]
pub async fn connect<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	connect_raw(socket, Ptr::from(addr).as_unit(), size_of::<A>() as u32).await
}

#[asynchronous]
pub async fn connect_addr(socket: BorrowedFd<'_>, addr: &Address) -> Result<()> {
	match &addr {
		Address::V4(addr) => connect(socket, addr).await,
		Address::V6(addr) => connect(socket, addr).await
	}
}

#[asynchronous]
pub async fn bind_addr(socket: BorrowedFd<'_>, addr: &Address) -> Result<()> {
	match &addr {
		Address::V4(addr) => bind(socket, addr).await,
		Address::V6(addr) => bind(socket, addr).await
	}
}

async_engine_task!(false, recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32) -> Result<usize>);

async_engine_task!(false, recvmsg(
	socket: BorrowedFd<'_>, header: &mut MsgHdr, flags: u32
) -> Result<usize>);

async_engine_task!(false, send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32) -> Result<usize>);

async_engine_task!(false, sendmsg(socket: BorrowedFd<'_>, header: &MsgHdr, flags: u32) -> Result<usize>);

async_engine_task!(false, shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()>);

use internal::bind as bind_raw;

#[asynchronous]
pub async fn bind<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	bind_raw(socket, Ptr::from(addr).as_unit(), size_of::<A>() as u32).await
}

async_engine_task!(false, listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()>);

async_engine_task!(false, fsync(file: BorrowedFd<'_>) -> Result<()>);

#[asynchronous]
pub async fn statx(path: &Path, flags: u32, mask: u32, statx: &mut Statx) -> Result<()> {
	let path = path_to_cstr(path)?;

	internal::statx(&path, flags, mask, statx).await
}

async_engine_task!(false, poll(fd: BorrowedFd<'_>, mask: u32) -> Result<u32>);
