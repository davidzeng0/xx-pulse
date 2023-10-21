use std::{
	io::{IoSlice, IoSliceMut},
	mem::{size_of, size_of_val},
	net::{SocketAddr, ToSocketAddrs},
	os::fd::{AsFd, BorrowedFd, OwnedFd}
};

use xx_core::{
	async_std::io::{Close, Read, Write},
	coroutines::{async_trait_fn, runtime::check_interrupt},
	error::{Error, ErrorKind, Result},
	os::{
		inet::{Address, AddressStorage, IpProtocol},
		iovec::IoVec,
		socket::{
			set_recvbuf_size, set_reuse_addr, set_sendbuf_size, set_tcp_keepalive, set_tcp_nodelay,
			AddressFamily, MessageHeader, Shutdown, SocketType, MAX_BACKLOG
		}
	},
	pointer::*
};

use crate::{async_runtime::*, ops::io::*};

#[async_fn]
async fn foreach_addr<A: ToSocketAddrs, F: Fn(&Address) -> Result<Output>, Output>(
	addrs: A, f: F
) -> Result<Output> {
	let mut error = None;

	for addr in addrs.to_socket_addrs()? {
		let addr = addr.into();

		match f(&addr) {
			Ok(out) => return Ok(out),
			Err(err) => error = Some(err)
		}

		check_interrupt().await?;
	}

	Err(error.unwrap_or(Error::new(ErrorKind::InvalidInput, "No addresses")))
}

#[async_fn]
async fn bind_addr<A: ToSocketAddrs>(addr: A, socket_type: u32, protocol: u32) -> Result<Socket> {
	foreach_addr(addr, |addr| {
		let socket = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		set_reuse_addr(socket.fd(), true)?;

		let result = match &addr {
			Address::V4(addr) => bind(socket.fd(), addr).await,
			Address::V6(addr) => bind(socket.fd(), addr).await
		};

		let err = match result {
			Ok(()) => return Ok(socket),
			Err(err) => err
		};

		socket.close().await?;

		Err(err)
	})
	.await
}

#[async_fn]
async fn connect_addrs<A: ToSocketAddrs>(
	addr: A, socket_type: u32, protocol: u32
) -> Result<Socket> {
	foreach_addr(addr, |addr| {
		let socket = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		set_reuse_addr(socket.fd(), true)?;

		let err = match socket.connect_addr(addr).await {
			Ok(()) => return Ok(socket),
			Err(err) => err
		};

		socket.close().await?;

		Err(err)
	})
	.await
}

fn convert_addr(storage: AddressStorage) -> SocketAddr {
	/* into should be safe here unless OS is broken */
	storage.try_into().unwrap()
}

pub struct Socket {
	fd: OwnedFd
}

#[async_fn]
impl Socket {
	pub async fn new(domain: u32, socket_type: u32, protocol: u32) -> Result<Socket> {
		let fd = socket(domain, socket_type, protocol).await?;

		Ok(Socket { fd })
	}

	pub async fn new_for_addr(addr: &Address, socket_type: u32, protocol: u32) -> Result<Socket> {
		match addr {
			Address::V4(_) => Self::new(AddressFamily::INet as u32, socket_type, protocol),
			Address::V6(_) => Self::new(AddressFamily::INet6 as u32, socket_type, protocol)
		}
		.await
	}

	pub fn fd(&self) -> BorrowedFd<'_> {
		self.fd.as_fd()
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}

	pub async fn connect_addr(&self, addr: &Address) -> Result<()> {
		match &addr {
			Address::V4(addr) => connect(self.fd(), addr).await,
			Address::V6(addr) => connect(self.fd(), addr).await
		}
	}

	pub async fn recv(&self, buf: &mut [u8], flags: u32) -> Result<usize> {
		recv(self.fd(), buf, flags).await
	}

	pub async fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
		let mut header = MessageHeader {
			iov: bufs.as_ptr() as *mut _,
			iov_len: bufs.len(),

			..Default::default()
		};

		recvmsg(self.fd(), &mut header, 0).await
	}

	pub async fn send(&self, buf: &[u8], flags: u32) -> Result<usize> {
		send(self.fd(), buf, flags).await
	}

	pub async fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> Result<usize> {
		let header = MessageHeader {
			iov: bufs.as_ptr() as *mut _,
			iov_len: bufs.len(),

			..Default::default()
		};

		sendmsg(self.fd(), &header, 0).await
	}

	pub async fn recvfrom(&self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)> {
		let mut addr = AddressStorage::new();
		let mut vec = IoVec { base: buf.as_ptr() as *const _, len: buf.len() };

		let mut header = MessageHeader {
			address: MutPtr::from(&mut addr).as_raw_ptr(),
			address_len: size_of::<AddressStorage>() as u32,

			iov: MutPtr::from(&mut vec).as_ptr_mut(),
			iov_len: 1,

			..Default::default()
		};

		let recvd = recvmsg(self.fd(), &mut header, flags).await?;

		Ok((recvd, convert_addr(addr)))
	}

	pub async fn sendto(&self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize> {
		let mut vec = IoVec { base: buf.as_ptr() as *const _, len: buf.len() };
		let addr = addr.clone().into();

		let (address, address_len) = {
			match &addr {
				Address::V4(addr) => (ConstPtr::from(addr).as_raw_ptr(), size_of_val(addr) as u32),
				Address::V6(addr) => (ConstPtr::from(addr).as_raw_ptr(), size_of_val(addr) as u32)
			}
		};

		let header = MessageHeader {
			address,
			address_len,

			iov: MutPtr::from(&mut vec).as_ptr_mut(),
			iov_len: 1,

			control: ConstPtr::<()>::null().as_ptr(),
			control_len: 0,

			flags: 0
		};

		sendmsg(self.fd(), &header, flags).await
	}

	pub async fn shutdown(&self, how: Shutdown) -> Result<()> {
		shutdown(self.fd(), how).await
	}

	pub async fn set_recvbuf_size(&self, size: i32) -> Result<()> {
		check_interrupt().await?;
		set_recvbuf_size(self.fd(), size)
	}

	pub async fn set_sendbuf_size(&self, size: i32) -> Result<()> {
		check_interrupt().await?;
		set_sendbuf_size(self.fd(), size)
	}

	pub async fn set_tcp_nodelay(&self, enable: bool) -> Result<()> {
		check_interrupt().await?;
		set_tcp_nodelay(self.fd(), enable)
	}

	pub async fn set_tcp_keepalive(&self, enable: bool, idle: i32) -> Result<()> {
		check_interrupt().await?;
		set_tcp_keepalive(self.fd(), enable, idle)
	}
}

impl From<OwnedFd> for Socket {
	fn from(fd: OwnedFd) -> Self {
		Self { fd }
	}
}

pub struct StreamSocket {
	socket: Socket
}

macro_rules! alias_func {
	($func: ident ($self: ident: $self_type: ty $(, $arg: ident: $type: ty)*) -> $return_type: ty) => {
		#[async_fn]
        pub async fn $func($self: $self_type $(, $arg: $type)*) -> $return_type {
			$self.socket.$func($($arg),*).await
        }
    }
}

macro_rules! socket_common {
	{} => {
		alias_func!(close(self: Self) -> Result<()>);

		alias_func!(recv(self: &Self, buf: &mut [u8], flags: u32) -> Result<usize>);

		alias_func!(read_vectored(self: &Self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize>);

		alias_func!(send(self: &Self, buf: &[u8], flags: u32) -> Result<usize>);

		alias_func!(write_vectored(self: &Self, bufs: &[IoSlice<'_>]) -> Result<usize>);

		alias_func!(recvfrom(self: &Self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)>);

		alias_func!(sendto(self: &Self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize>);

		alias_func!(shutdown(self: &Self, how: Shutdown) -> Result<()>);

		alias_func!(set_recvbuf_size(self: &Self, size: i32) -> Result<()>);

		alias_func!(set_sendbuf_size(self: &Self, size: i32) -> Result<()>);
	}
}

macro_rules! socket_impl {
	($struct: ident) => {
		#[async_trait_fn]
		impl Read<Context> for $struct {
			async fn async_read(&mut self, buf: &mut [u8]) -> Result<usize> {
				self.recv(buf, 0).await
			}

			fn is_read_vectored(&self) -> bool {
				true
			}

			async fn async_read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
				self.read_vectored(bufs).await
			}
		}

		#[async_trait_fn]
		impl Write<Context> for $struct {
			async fn async_write(&mut self, buf: &[u8]) -> Result<usize> {
				self.send(buf, 0).await
			}

			async fn async_flush(&mut self) -> Result<()> {
				/* sockets don't need flushing. set nodelay if you want immediate writes */
				Ok(())
			}

			fn is_write_vectored(&self) -> bool {
				true
			}

			async fn async_write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<usize> {
				self.write_vectored(bufs).await
			}
		}

		#[async_trait_fn]
		impl Close<Context> for $struct {
			async fn async_close(self) -> Result<()> {
				self.close().await
			}
		}
	};
}

impl StreamSocket {
	socket_common! {}

	alias_func!(set_tcp_nodelay(self: &Self, enable: bool) -> Result<()>);

	alias_func!(set_tcp_keepalive(self: &Self, enable: bool, idle: i32) -> Result<()>);
}

socket_impl!(StreamSocket);

pub struct DatagramSocket {
	socket: Socket
}

impl DatagramSocket {
	socket_common! {}

	alias_func!(connect_addr(self: &Self, addr: &Address) -> Result<()>);

	#[async_fn]
	pub async fn connect<A: ToSocketAddrs>(&self, addrs: A) -> Result<()> {
		foreach_addr(addrs, |addr| self.socket.connect_addr(addr).await).await
	}
}

socket_impl!(DatagramSocket);

pub struct TcpListener {
	socket: Socket
}

impl TcpListener {
	alias_func!(close(self: Self) -> Result<()>);

	#[async_fn]
	pub async fn accept(&self) -> Result<(StreamSocket, SocketAddr)> {
		let mut storage = AddressStorage::new();
		let (fd, _) = accept(self.socket.fd(), &mut storage).await?;

		Ok((
			StreamSocket { socket: Socket { fd } },
			convert_addr(storage)
		))
	}
}

pub struct Tcp;

#[async_fn]
impl Tcp {
	pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<StreamSocket> {
		let socket = connect_addrs(addr, SocketType::Stream as u32, IpProtocol::Tcp as u32).await?;

		Ok(StreamSocket { socket })
	}

	pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<TcpListener> {
		let socket = bind_addr(addr, SocketType::Stream as u32, IpProtocol::Tcp as u32).await?;

		listen(socket.fd(), MAX_BACKLOG).await?;

		Ok(TcpListener { socket })
	}
}

pub struct Udp;

#[async_fn]
impl Udp {
	pub async fn connect<A: ToSocketAddrs>(addrs: A) -> Result<DatagramSocket> {
		let socket =
			connect_addrs(addrs, SocketType::Datagram as u32, IpProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket })
	}

	pub async fn bind<A: ToSocketAddrs>(addrs: A) -> Result<DatagramSocket> {
		let socket = bind_addr(addrs, SocketType::Datagram as u32, IpProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket })
	}
}
