use std::{
	io::{Error, ErrorKind, Result},
	mem::{size_of, size_of_val},
	net::{SocketAddr, ToSocketAddrs},
	os::fd::{AsFd, BorrowedFd, OwnedFd}
};

use xx_core::{
	os::{
		inet::{Address, AddressStorage, IPProtocol},
		iovec::IoVec,
		socket::{set_reuse_addr, AddressFamily, MessageHeader, Shutdown, SocketType, MAX_BACKLOG}
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
async fn bind_addr<A: ToSocketAddrs>(addr: A, socket_type: u32, protocol: u32) -> Result<Socket> {
	let mut error = None;

	for addr in addr.to_socket_addrs()? {
		let addr = addr.into();
		let socket = new_socket_for_addr(&addr, socket_type, protocol).await?;

		set_reuse_addr(socket.fd(), true)?;

		let result = match &addr {
			Address::V4(addr) => bind(socket.fd(), addr).await,
			Address::V6(addr) => bind(socket.fd(), addr).await
		};

		match result {
			Err(err) => error = Some(err),
			Ok(()) => return Ok(socket)
		}
	}

	Err(error.unwrap_or(Error::new(ErrorKind::InvalidInput, "No addresses")))
}

#[async_fn]
async fn connect_addr<A: ToSocketAddrs>(
	addr: A, socket_type: u32, protocol: u32
) -> Result<Socket> {
	let mut error = None;

	for addr in addr.to_socket_addrs()? {
		let addr = addr.into();
		let socket = new_socket_for_addr(&addr, socket_type, protocol).await?;

		let result = match &addr {
			Address::V4(addr) => connect(socket.fd(), addr).await,
			Address::V6(addr) => connect(socket.fd(), addr).await
		};

		match result {
			Err(err) => error = Some(err),
			Ok(()) => return Ok(socket)
		}
	}

	Err(error.unwrap_or(Error::new(ErrorKind::InvalidInput, "No addresses")))
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
}

pub struct StreamSocket {
	socket: Socket
}

#[async_fn]
impl StreamSocket {
	pub async fn recv(&self, buf: &mut [u8], flags: u32) -> Result<usize> {
		recv(self.socket.fd(), buf, flags).await
	}

	pub async fn send(&self, buf: &[u8], flags: u32) -> Result<usize> {
		send(self.socket.fd(), buf, flags).await
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

		let recvd = recvmsg(self.socket.fd(), &mut header, flags).await?;

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

		sendmsg(self.socket.fd(), &header, flags).await
	}

	pub async fn shutdown(&self, how: Shutdown) -> Result<()> {
		shutdown(self.socket.fd(), how).await
	}

	pub async fn close(self) -> Result<()> {
		self.socket.close().await
	}
}

pub struct TcpListener {
	socket: Socket
}

#[async_fn]
impl TcpListener {
	pub async fn accept(&self) -> Result<(StreamSocket, SocketAddr)> {
		let mut storage = AddressStorage::new();
		let (fd, _) = accept(self.socket.fd(), &mut storage).await?;

		Ok((
			StreamSocket { socket: Socket { fd } },
			convert_addr(storage)
		))
	}

	pub async fn close(self) -> Result<()> {
		self.socket.close().await
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
	pub async fn connect(addr: &str) -> Result<StreamSocket> {
		let socket =
			connect_addr(addr, SocketType::Datagram as u32, IPProtocol::Udp as u32).await?;

		Ok(StreamSocket { socket })
	}

	pub async fn bind(addr: &str) -> Result<StreamSocket> {
		let socket = bind_addr(addr, SocketType::Datagram as u32, IPProtocol::Udp as u32).await?;

		Ok(StreamSocket { socket })
	}
}
