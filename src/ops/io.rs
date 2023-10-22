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
	coroutines::runtime::*,
	error::*,
	opt::hint::unlikely,
	os::{
		error::ErrorCodes,
		socket::{MessageHeader, Shutdown},
		stat::Statx
	},
	pointer::*
};

use super::*;
use crate::{driver::driver_shutdown_error, engine::Engine};

#[inline(never)]
fn check_exiting(driver: Handle<Driver>, err: &Error) -> Result<()> {
	if driver.exiting() && err.os_error() == Some(ErrorCodes::Nxio) {
		Err(driver_shutdown_error())
	} else {
		Ok(())
	}
}

macro_rules! async_engine_task {
	($func: ident ($($arg: ident: $type: ty),*) -> $return_type: ty) => {
		#[async_fn]
		#[inline(always)]
		pub async fn $func($($arg: $type),*) -> $return_type {
			check_interrupt().await?;

			let result = block_on(internal_get_driver().await.$func($($arg),*)).await;
			let result = paste::paste! { Engine::[<result_for_ $func>](result) };

			if unlikely(result.is_err()) {
				check_exiting(internal_get_driver().await, result.as_ref().unwrap_err())?;
			}

			result
		}
	}
}

fn path_to_cstr(path: &Path) -> Result<CString> {
	CString::new(path.as_os_str().as_bytes())
		.map_err(|_| Error::new(ErrorKind::InvalidInput, "Path string contained a null byte"))
}

mod internal {
	use super::*;

	async_engine_task!(open(path: &CStr, flags: u32, mode: u32) -> Result<OwnedFd>);

	async_engine_task!(accept(
		socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32
	) -> Result<OwnedFd>);

	async_engine_task!(connect(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()>);

	async_engine_task!(bind(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()>);

	async_engine_task!(statx(path: &CStr, flags: u32, mask: u32, statx: &mut Statx) -> Result<()>);
}

#[async_fn]
pub async fn open(path: &Path, flags: u32, mode: u32) -> Result<OwnedFd> {
	check_interrupt().await?;

	let path = path_to_cstr(path)?;

	internal::open(&path, flags, mode).await
}

async_engine_task!(close(fd: OwnedFd) -> Result<()>);
async_engine_task!(read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64) -> Result<usize>);
async_engine_task!(write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64) -> Result<usize>);
async_engine_task!(socket(domain: u32, socket_type: u32, protocol: u32) -> Result<OwnedFd>);

use internal::accept as accept_raw;

#[async_fn]
pub async fn accept<A>(socket: BorrowedFd<'_>, addr: &mut A) -> Result<(OwnedFd, u32)> {
	let mut addrlen = size_of::<A>() as u32;
	let fd = accept_raw(socket, MutPtr::from(addr).cast(), &mut addrlen).await?;

	Ok((fd, addrlen))
}

use internal::connect as connect_raw;

#[async_fn]
pub async fn connect<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	connect_raw(socket, ConstPtr::from(addr).cast(), size_of::<A>() as u32).await
}

async_engine_task!(recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32) -> Result<usize>);
async_engine_task!(recvmsg(
	socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32
) -> Result<usize>);
async_engine_task!(send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32) -> Result<usize>);

async_engine_task!(sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32) -> Result<usize>);

async_engine_task!(shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()>);

use internal::bind as bind_raw;

#[async_fn]
pub async fn bind<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	bind_raw(socket, ConstPtr::from(addr).cast(), size_of::<A>() as u32).await
}

async_engine_task!(listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()>);

async_engine_task!(fsync(file: BorrowedFd<'_>) -> Result<()>);

#[async_fn]
pub async fn statx(path: &Path, flags: u32, mask: u32, statx: &mut Statx) -> Result<()> {
	check_interrupt().await?;

	let path = path_to_cstr(path)?;

	internal::statx(&path, flags, mask, statx).await
}

async_engine_task!(poll(fd: BorrowedFd<'_>, mask: u32) -> Result<u32>);
