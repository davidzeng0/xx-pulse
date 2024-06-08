use std::{
	ffi::CString,
	mem::size_of,
	os::{
		fd::{AsRawFd, BorrowedFd, OwnedFd, RawFd},
		unix::prelude::OsStrExt
	},
	path::Path
};

use xx_core::{
	error::*,
	os::{epoll::*, fcntl::*, inet::*, openat::*, socket::*, stat::*},
	paste::paste,
	pointer::*
};

use super::*;

pub mod raw {
	use xx_core::os::socket::raw::MsgHdr;

	use super::*;

	#[cfg(feature = "tracing")]
	#[allow(unreachable_pub)]
	mod tracing {
		use std::{fmt, marker::PhantomData};

		use enumflags2::*;
		pub use xx_core::{num_traits::FromPrimitive, pointer::*};

		pub unsafe fn get_cstr_as_str<'a>(cstr: Ptr<()>) -> &'a str {
			/* Safety: guaranteed by caller */
			let cstr = unsafe { std::ffi::CStr::from_ptr(cstr.as_ptr().cast()) };

			cstr.to_str().unwrap_or("<error>")
		}

		pub struct EnumDisplay<T>(u32, PhantomData<T>);

		impl<T> EnumDisplay<T> {
			pub const fn new(value: u32) -> Self {
				Self(value, PhantomData)
			}
		}

		impl<T: FromPrimitive + fmt::Debug> fmt::Display for EnumDisplay<T> {
			fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
				if let Some(val) = T::from_u32(self.0) {
					fmt::Debug::fmt(&val, fmt)
				} else {
					fmt::Display::fmt(&self.0, fmt)
				}
			}
		}

		pub struct FlagsDisplay<T>(u32, PhantomData<T>);

		impl<T> FlagsDisplay<T> {
			pub const fn new(value: u32) -> Self {
				Self(value, PhantomData)
			}
		}

		impl<T: BitFlag<Numeric = u32> + Clone + fmt::Debug> fmt::Display for FlagsDisplay<T> {
			fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
				let (flags, invalid) = match BitFlags::<T>::from_bits(self.0) {
					Ok(flags) => (flags, None),
					Err(err) => (err.truncate(), Some(err.invalid_bits()))
				};

				match (flags.is_empty(), invalid) {
					(_, None) => fmt::Display::fmt(&flags, fmt)?,
					(false, Some(invalid)) => fmt::LowerHex::fmt(&invalid, fmt)?,
					(true, Some(invalid)) => {
						fmt::Display::fmt(&flags, fmt)?;
						fmt::Display::fmt(&" + ", fmt)?;
						fmt::LowerHex::fmt(&invalid, fmt)?;
					}
				}

				Ok(())
			}
		}

		impl<T: BitFlag<Numeric = u32> + Clone + fmt::Debug> fmt::Debug for FlagsDisplay<T> {
			fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
				fmt::Display::fmt(self, fmt)
			}
		}
	}

	#[cfg(feature = "tracing")]
	use tracing::*;

	macro_rules! async_engine_task {
		($force: literal, $func: ident ($($arg: ident: $type: ty),*) -> $return_type: ty {
			trace($($trace:tt)*) = $result:ident $($map:tt)*
		}) => {
			/// # Safety
			/// all pointers must be valid until the function returns
			#[asynchronous]
			#[inline(always)]
			pub async unsafe fn $func($($arg: $type),*) -> $return_type {
				let driver = internal_get_driver().await;

				if !$force {
					check_interrupt().await?;

					driver.check_exiting()?;
				}

				/* Safety: guaranteed by caller */
				let result = unsafe { block_on(driver.$func($($arg),*)).await };
				let $result = paste! { Engine::[<result_for_ $func>](result) };

				#[cfg(feature = "tracing")]
				xx_core::trace!(target: driver, $($trace)*, $result $($map)*);

				$result.map_err(|err| err.into())
			}
		}
	}

	async_engine_task!(false, open(path: Ptr<()>, flags: u32, mode: u32) -> Result<OwnedFd> {
		trace(
			"## open(path = {}, flags = {}, mode = {:o}) = {:?}",
			/* Safety: guaranteed by caller */
			unsafe { get_cstr_as_str(path) },
			FlagsDisplay::<OpenFlag>::new(flags),
			mode
		) = result
	});

	async_engine_task!(true, close(fd: RawFd) -> Result<()> {
		trace("## close(fd = {}) = {:?}", fd) = result
	});

	async_engine_task!(false, read(fd: RawFd, buf: MutPtr<()>, len: usize, offset: i64) -> Result<usize> {
		trace("## read(fd = {}, buf = &mut [u8; {}], offset = {}) = {:?}", fd, len, offset) = result
	});

	async_engine_task!(false, write(fd: RawFd, buf: Ptr<()>, len: usize, offset: i64) -> Result<usize> {
		trace("## write(fd = {}, buf = &[u8; {}], offset = {}) = {:?}", fd, len, offset) = result
	});

	async_engine_task!(false, socket(domain: u32, socket_type: u32, protocol: u32) -> Result<OwnedFd> {
		trace(
			"## socket(domain = {}, socket_type = {}, protocol = {}) = {:?}",
			EnumDisplay::<AddressFamily>::new(domain),
			EnumDisplay::<SocketType>::new(socket_type),
			EnumDisplay::<IpProtocol>::new(protocol)
		) = result
	});

	async_engine_task!(false, accept(socket: RawFd, addr: MutPtr<()>, addrlen: MutPtr<i32>) -> Result<OwnedFd> {
		trace("## accept(fd = {}, addr = {:?}, addrlen = {:?}) = {:?}", socket, addr, addrlen) = result
	});

	async_engine_task!(false, connect(socket: RawFd, addr: Ptr<()>, addrlen: i32) -> Result<()> {
		trace("## connect(fd = {}, addr = {:?}, addrlen = {}) = {:?}", socket, addr, addrlen) = result
	});

	async_engine_task!(false, recv(socket: RawFd, buf: MutPtr<()>, len: usize, flags: u32) -> Result<usize> {
		trace(
			"## recv(fd = {}, buf = &mut [u8; {}], flags = {}) = {:?}",
			socket,
			len,
			FlagsDisplay::<MessageFlag>::new(flags)
		) = result
	});

	async_engine_task!(false, recvmsg(socket: RawFd, header: MutPtr<MsgHdr>, flags: u32) -> Result<usize> {
		trace(
			"## recvmsg(fd = {}, header = {:?}, flags = {}) = {:?}",
			socket,
			header,
			FlagsDisplay::<MessageFlag>::new(flags)
		) = result
	});

	async_engine_task!(false, send(socket: RawFd, buf: Ptr<()>, len: usize, flags: u32) -> Result<usize> {
		trace(
			"## send(fd = {}, buf = &[u8; {}], flags = {}) = {:?}",
			socket,
			len,
			FlagsDisplay::<MessageFlag>::new(flags)
		) = result
	});

	async_engine_task!(false, sendmsg(socket: RawFd, header: Ptr<MsgHdr>, flags: u32) -> Result<usize> {
		trace(
			"## sendmsg(fd = {}, header = {:?}, flags = {}) = {:?}",
			socket,
			header,
			FlagsDisplay::<MessageFlag>::new(flags)
		) = result
	});

	async_engine_task!(false, shutdown(socket: RawFd, how: u32) -> Result<()> {
		trace("## shutdown(fd = {}, how = {:?}) = {:?}", socket, Shutdown::from_u32(how)) = result
	});

	async_engine_task!(false, bind(socket: RawFd, addr: Ptr<()>, addrlen: i32) -> Result<()> {
		trace("## bind(fd = {}, addr = {:?}, addrlen = {}) = {:?}", socket, addr, addrlen) = result
	});

	async_engine_task!(false, listen(socket: RawFd, backlog: i32) -> Result<()> {
		trace("## listen(fd = {}, backlog = {}) = {:?}", socket, backlog) = result
	});

	async_engine_task!(false, fsync(file: RawFd) -> Result<()> {
		trace("## fsync(fd = {}) = {:?}", file) = result
	});

	async_engine_task!(false, statx(dirfd: RawFd, path: Ptr<()>, flags: u32, mask: u32, statx: MutPtr<Statx>) -> Result<()> {
		trace(
			"## statx(dirfd = {}, path = {}, flags = {}, mask = {}, statx = {:?}) = {:?}",
			dirfd,
			/* Safety: guaranteed by caller */
			unsafe { get_cstr_as_str(path) },
			FlagsDisplay::<AtFlag>::new(flags),
			FlagsDisplay::<StatxMask>::new(mask),
			statx
		) = result
	});

	async_engine_task!(false, poll(fd: RawFd, mask: u32) -> Result<u32> {
		trace("## poll(fd = {}, mask = {}) = {:?}", fd, FlagsDisplay::<PollFlag>::new(mask)) = result
			.as_ref()
			.map(|mask| FlagsDisplay::<PollFlag>::new(*mask))
	});
}

fn path_to_cstr(path: impl AsRef<Path>) -> Result<CString> {
	CString::new(path.as_ref().as_os_str().as_bytes()).map_err(|_| ErrorKind::invalid_cstr().into())
}

#[asynchronous]
#[allow(clippy::impl_trait_in_params)]
pub async fn open(path: impl AsRef<Path>, flags: BitFlags<OpenFlag>, mode: u32) -> Result<OwnedFd> {
	let path = path_to_cstr(path)?;

	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::open(ptr!(path.as_ptr()).cast(), flags.bits(), mode).await }
}

#[asynchronous]
pub async fn close(fd: OwnedFd) -> Result<()> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::close(fd.as_raw_fd()).await }
}

#[asynchronous]
pub async fn read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64) -> Result<usize> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe {
		raw::read(
			fd.as_raw_fd(),
			ptr!(buf.as_mut_ptr()).cast(),
			buf.len(),
			offset
		)
		.await
	}
}

#[asynchronous]
pub async fn write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64) -> Result<usize> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::write(fd.as_raw_fd(), ptr!(buf.as_ptr()).cast(), buf.len(), offset).await }
}

#[asynchronous]
pub async fn socket(
	domain: AddressFamily, socket_type: u32, protocol: IpProtocol
) -> Result<OwnedFd> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::socket(domain as u32, socket_type, protocol as u32).await }
}

#[asynchronous]
pub async fn accept<A>(socket: BorrowedFd<'_>, addr: &mut A) -> Result<(OwnedFd, i32)> {
	#[allow(clippy::unwrap_used)]
	let mut addrlen = size_of::<A>().try_into().unwrap();

	/* Safety: lifetimes captured by this function are valid until it returns */
	let fd =
		unsafe { raw::accept(socket.as_raw_fd(), ptr!(addr).cast(), ptr!(&mut addrlen)).await? };

	Ok((fd, addrlen))
}

#[asynchronous]
pub async fn connect<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	#[allow(clippy::unwrap_used)]
	unsafe {
		raw::connect(
			socket.as_raw_fd(),
			ptr!(addr).cast(),
			size_of::<A>().try_into().unwrap()
		)
		.await
	}
}

#[asynchronous]
pub async fn connect_addr(socket: BorrowedFd<'_>, addr: &Address) -> Result<()> {
	match &addr {
		Address::V4(addr) => connect(socket, addr).await,
		Address::V6(addr) => connect(socket, addr).await
	}
}

#[asynchronous]
pub async fn recv(
	socket: BorrowedFd<'_>, buf: &mut [u8], flags: BitFlags<MessageFlag>
) -> Result<usize> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe {
		raw::recv(
			socket.as_raw_fd(),
			ptr!(buf.as_mut_ptr()).cast(),
			buf.len(),
			flags.bits()
		)
		.await
	}
}

#[asynchronous]
pub async fn recvmsg(
	socket: BorrowedFd<'_>, header: &mut MsgHdrMut<'_>, flags: BitFlags<MessageFlag>
) -> Result<usize> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::recvmsg(socket.as_raw_fd(), ptr!(header).cast(), flags.bits()).await }
}

#[asynchronous]
pub async fn send(
	socket: BorrowedFd<'_>, buf: &[u8], flags: BitFlags<MessageFlag>
) -> Result<usize> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe {
		raw::send(
			socket.as_raw_fd(),
			ptr!(buf.as_ptr()).cast(),
			buf.len(),
			flags.bits()
		)
		.await
	}
}

#[asynchronous]
pub async fn sendmsg(
	socket: BorrowedFd<'_>, header: &MsgHdr<'_>, flags: BitFlags<MessageFlag>
) -> Result<usize> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::sendmsg(socket.as_raw_fd(), ptr!(header).cast(), flags.bits()).await }
}

#[asynchronous]
pub async fn shutdown(socket: BorrowedFd<'_>, how: Shutdown) -> Result<()> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::shutdown(socket.as_raw_fd(), how as u32).await }
}

#[asynchronous]
pub async fn bind<A>(socket: BorrowedFd<'_>, addr: &A) -> Result<()> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	#[allow(clippy::unwrap_used)]
	unsafe {
		raw::bind(
			socket.as_raw_fd(),
			ptr!(addr).cast(),
			size_of::<A>().try_into().unwrap()
		)
		.await
	}
}

#[asynchronous]
pub async fn bind_addr(socket: BorrowedFd<'_>, addr: &Address) -> Result<()> {
	match &addr {
		Address::V4(addr) => bind(socket, addr).await,
		Address::V6(addr) => bind(socket, addr).await
	}
}

#[asynchronous]
pub async fn listen(socket: BorrowedFd<'_>, backlog: i32) -> Result<()> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::listen(socket.as_raw_fd(), backlog).await }
}

#[asynchronous]
pub async fn fsync(file: BorrowedFd<'_>) -> Result<()> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe { raw::fsync(file.as_raw_fd()).await }
}

#[asynchronous]
#[allow(clippy::impl_trait_in_params)]
pub async fn statx(
	dirfd: Option<BorrowedFd<'_>>, path: impl AsRef<Path>, flags: BitFlags<AtFlag>,
	mask: BitFlags<StatxMask>, statx: &mut Statx
) -> Result<()> {
	let dirfd = dirfd.map_or(OpenAt::CurrentWorkingDirectory as i32, |fd| fd.as_raw_fd());
	let path = path_to_cstr(path)?;

	/* Safety: lifetimes captured by this function are valid until it returns */
	unsafe {
		raw::statx(
			dirfd,
			ptr!(path.as_ptr()).cast(),
			flags.bits(),
			mask.bits(),
			statx.into()
		)
		.await
	}
}

#[asynchronous]
pub async fn poll(fd: BorrowedFd<'_>, mask: BitFlags<PollFlag>) -> Result<BitFlags<PollFlag>> {
	/* Safety: lifetimes captured by this function are valid until it returns */
	let bits = unsafe { raw::poll(fd.as_raw_fd(), mask.bits()).await? };

	Ok(BitFlags::from_bits_truncate(bits))
}
