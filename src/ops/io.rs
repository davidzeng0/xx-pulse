use std::{
	ffi::CString,
	mem::size_of,
	os::{
		fd::{BorrowedFd, FromRawFd, OwnedFd},
		unix::prelude::OsStrExt
	},
	path::Path
};

use xx_core::{
	coroutines::runtime::{block_on, check_interrupt},
	error::{Error, ErrorKind, Result},
	os::{
		socket::{MessageHeader, Shutdown},
		stat::Statx
	},
	pointer::{ConstPtr, MutPtr}
};

use super::*;

fn path_to_cstr(path: &Path) -> Result<CString> {
	CString::new(path.as_os_str().as_bytes())
		.map_err(|_| Error::new(ErrorKind::InvalidInput, "Path string contained a null byte"))
}

#[async_fn]
pub async fn open(path: &Path, flags: u32, mode: u32) -> Result<OwnedFd> {
	check_interrupt().await?;

	let path = path_to_cstr(path)?;
	let fd = block_on(internal_get_driver().await.open(path.as_ref(), flags, mode)).await?;

	Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

#[async_fn]
pub async fn close(fd: OwnedFd) -> Result<()> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.close(fd)).await?;

	Ok(())
}

#[async_fn]
#[inline(always)]
pub async fn read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64) -> Result<usize> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.read(fd, buf, offset)).await
}

#[async_fn]
#[inline(always)]
pub async fn write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64) -> Result<usize> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.write(fd, buf, offset)).await
}

#[async_fn]
pub async fn socket(domain: u32, socket_type: u32, protocol: u32) -> Result<OwnedFd> {
	check_interrupt().await?;

	let fd = block_on(
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
	check_interrupt().await?;

	let fd = block_on(internal_get_driver().await.accept(socket, addr, addrlen)).await?;

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
	check_interrupt().await?;
	block_on(internal_get_driver().await.connect(socket, addr, addrlen)).await?;

	Ok(())
}

#[async_fn]
pub async fn connect<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	connect_raw(socket, ConstPtr::from(addr).cast(), size_of::<A>() as u32).await
}

#[async_fn]
#[inline(always)]
pub async fn recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32) -> Result<usize> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.recv(socket, buf, flags)).await
}

#[async_fn]
#[inline(always)]
pub async fn recvmsg(
	socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32
) -> Result<usize> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.recvmsg(socket, header, flags)).await
}

#[async_fn]
#[inline(always)]
pub async fn send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32) -> Result<usize> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.send(socket, buf, flags)).await
}

#[async_fn]
#[inline(always)]
pub async fn sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32) -> Result<usize> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.sendmsg(socket, header, flags)).await
}

#[async_fn]
pub async fn shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.shutdown(socket, how)).await?;

	Ok(())
}

#[async_fn]
pub async fn bind_raw(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32) -> Result<()> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.bind(socket, addr, addrlen)).await?;

	Ok(())
}

#[async_fn]
pub async fn bind<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	bind_raw(socket, ConstPtr::from(addr).cast(), size_of::<A>() as u32).await
}

#[async_fn]
pub async fn listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.listen(socket, backlog)).await?;

	Ok(())
}

#[async_fn]
pub async fn fsync(file: BorrowedFd<'_>) -> Result<()> {
	check_interrupt().await?;
	block_on(internal_get_driver().await.fsync(file)).await?;

	Ok(())
}

#[async_fn]
pub async fn statx(path: &Path, flags: u32, mask: u32, statx: &mut Statx) -> Result<()> {
	check_interrupt().await?;

	let path = path_to_cstr(path)?;

	block_on(internal_get_driver().await.statx(&path, flags, mask, statx)).await?;

	Ok(())
}
