use std::{
	io::{Error, ErrorKind, Result},
	mem::{size_of, size_of_val},
	net::{SocketAddr, ToSocketAddrs},
	os::fd::{AsFd, BorrowedFd, OwnedFd}
};

use xx_core::{
	async_std::io::{Close, Read, Write},
	coroutines::async_trait_fn,
	os::{
		inet::{Address, AddressStorage, IPProtocol},
		iovec::IoVec,
		socket::{
			set_recvbuf_size, set_reuse_addr, set_sendbuf_size, set_tcp_keepalive, AddressFamily,
			MessageHeader, Shutdown, SocketType, MAX_BACKLOG
		}
	},
	pointer::*
};

use crate::{async_runtime::*, ops::io::*};

#[async_fn]
async fn new_socket_for_addr(addr: &Address, socket_type: u32, protocol: u32) -> Result<Socket> {
	let fd = match addr {
		Address::V4(_) => socket(AddressFamily::INet as u32, socket_type, protocol),
		Address::V6(_) => socket(AddressFamily::INet6 as u32, socket_type, protocol)
	}
	.await?;

	Ok(Socket { fd })
}

#[async_fn]
async fn foreach_addr<A: ToSocketAddrs, F: Fn(&Socket, &Address) -> Result<()>>(
	addrs: A, socket_type: u32, protocol: u32, f: F
) -> Result<Socket> {
	let mut error = None;

	for addr in addrs.to_socket_addrs()? {
		let addr = addr.into();
		let socket = new_socket_for_addr(&addr, socket_type, protocol).await?;

		match f(&socket, &addr) {
			Ok(()) => return Ok(socket),
			Err(err) => error = Some(err)
		}

		socket.close().await?;
	}

	Err(error.unwrap_or(Error::new(ErrorKind::InvalidInput, "No addresses")))
}

#[async_fn]
async fn bind_addr<A: ToSocketAddrs>(addr: A, socket_type: u32, protocol: u32) -> Result<Socket> {
	foreach_addr(addr, socket_type, protocol, |socket, addr| {
		set_reuse_addr(socket.fd(), true)?;

		match &addr {
			Address::V4(addr) => bind(socket.fd(), addr).await,
			Address::V6(addr) => bind(socket.fd(), addr).await
		}
	})
	.await
}

#[async_fn]
async fn connect_addr<A: ToSocketAddrs>(
	addr: A, socket_type: u32, protocol: u32
) -> Result<Socket> {
	foreach_addr(addr, socket_type, protocol, |socket, addr| {
		set_reuse_addr(socket.fd(), true)?;

		match &addr {
			Address::V4(addr) => connect(socket.fd(), addr).await,
			Address::V6(addr) => connect(socket.fd(), addr).await
		}
	})
	.await
}

fn convert_addr(storage: AddressStorage) -> SocketAddr {
	/* into should be safe here unless OS is broken */
	storage.try_into().unwrap()
}

struct Socket {
	fd: OwnedFd
}

#[async_fn]
impl Socket {
	pub fn fd(&self) -> BorrowedFd<'_> {
		self.fd.as_fd()
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}

	pub async fn recv(&self, buf: &mut [u8], flags: u32) -> Result<usize> {
		recv(self.fd(), buf, flags).await
	}

	pub async fn send(&self, buf: &[u8], flags: u32) -> Result<usize> {
		send(self.fd(), buf, flags).await
	}

	pub async fn recvfrom(&self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)> {
		let mut addr = AddressStorage::new();
		let mut vec = IoVec { base: buf.as_ptr() as *const _, len: buf.len() };

		let mut header = MessageHeader {
			address: MutPtr::from(&mut addr).as_raw_ptr(),
			address_len: size_of::<AddressStorage>() as u32,

			iov: MutPtr::from(&mut vec).as_ptr_mut(),
			iov_len: 1,

			control: ConstPtr::<()>::null().as_ptr(),
			control_len: 0,

			flags: 0
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
		set_recvbuf_size(self.fd(), size)
	}

	pub async fn set_sendbuf_size(&self, size: i32) -> Result<()> {
		set_sendbuf_size(self.fd(), size)
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

impl StreamSocket {
	alias_func!(close(self: Self) -> Result<()>);

	alias_func!(recv(self: &Self, buf: &mut [u8], flags: u32) -> Result<usize>);

	alias_func!(send(self: &Self, buf: &[u8], flags: u32) -> Result<usize>);

	alias_func!(recvfrom(self: &Self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)>);

	alias_func!(sendto(self: &Self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize>);

	alias_func!(shutdown(self: &Self, how: Shutdown) -> Result<()>);

	alias_func!(set_recvbuf_size(self: &Self, size: i32) -> Result<()>);

	alias_func!(set_sendbuf_size(self: &Self, size: i32) -> Result<()>);

	pub async fn set_tcp_nodelay(&self, size: i32) -> Result<()> {
		set_sendbuf_size(self.socket.fd(), size)
	}

	pub async fn set_tcp_keepalive(&self, enable: bool, idle: i32) -> Result<()> {
		set_tcp_keepalive(self.socket.fd(), enable, idle)
	}
}

#[async_trait_fn]
impl Read<Context> for StreamSocket {
	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.recv(buf, 0).await
	}
}

#[async_trait_fn]
impl Write<Context> for StreamSocket {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.send(buf, 0).await
	}

	async fn flush(&mut self) -> Result<()> {
		/* sockets don't need flushing. set nodelay if you want immediate writes */
		Ok(())
	}
}

#[async_trait_fn]
impl Close<Context> for StreamSocket {
	async fn close(self) -> Result<()> {
		self.close().await
	}
}

pub struct DatagramSocket {
	socket: Socket
}

impl DatagramSocket {
	alias_func!(close(self: Self) -> Result<()>);

	alias_func!(recv(self: &Self, buf: &mut [u8], flags: u32) -> Result<usize>);

	alias_func!(send(self: &Self, buf: &[u8], flags: u32) -> Result<usize>);

	alias_func!(recvfrom(self: &Self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)>);

	alias_func!(sendto(self: &Self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize>);

	alias_func!(shutdown(self: &Self, how: Shutdown) -> Result<()>);

	alias_func!(set_recvbuf_size(self: &Self, size: i32) -> Result<()>);

	alias_func!(set_sendbuf_size(self: &Self, size: i32) -> Result<()>);
}

#[async_trait_fn]
impl Read<Context> for DatagramSocket {
	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.recv(buf, 0).await
	}
}

#[async_trait_fn]
impl Write<Context> for DatagramSocket {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.send(buf, 0).await
	}

	async fn flush(&mut self) -> Result<()> {
		Ok(())
	}
}

#[async_trait_fn]
impl Close<Context> for DatagramSocket {
	async fn close(self) -> Result<()> {
		self.close().await
	}
}

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
		let socket = connect_addr(addr, SocketType::Stream as u32, IPProtocol::Tcp as u32).await?;

		Ok(StreamSocket { socket })
	}

	pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<TcpListener> {
		let socket = bind_addr(addr, SocketType::Stream as u32, IPProtocol::Tcp as u32).await?;

		listen(socket.fd(), MAX_BACKLOG).await?;

		Ok(TcpListener { socket })
	}
}

pub struct Udp;

#[async_fn]
impl Udp {
	pub async fn connect(addr: &str) -> Result<DatagramSocket> {
		let socket =
			connect_addr(addr, SocketType::Datagram as u32, IPProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket })
	}

	pub async fn bind(addr: &str) -> Result<DatagramSocket> {
		let socket = bind_addr(addr, SocketType::Datagram as u32, IPProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket })
	}
}
