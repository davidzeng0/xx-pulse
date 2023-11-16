use std::{
	mem::{size_of, size_of_val},
	net::{SocketAddr, ToSocketAddrs}
};

use xx_core::{
	os::{inet::*, socket::*},
	pointer::*,
	read_wrapper, wrapper_functions, write_wrapper
};

use super::*;

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
	}

	Err(error.unwrap_or_else(|| Error::new(ErrorKind::InvalidInput, "Address list empty")))
}

#[async_fn]
async fn bind_addr<A: ToSocketAddrs>(addr: A, socket_type: u32, protocol: u32) -> Result<Socket> {
	foreach_addr(addr, |addr| {
		let sock = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		set_reuse_addr(sock.fd(), true)?;

		ops::bind_addr(sock.fd(), addr).await?;
		Ok(sock)
	})
	.await
}

#[async_fn]
async fn connect_addrs<A: ToSocketAddrs>(
	addr: A, socket_type: u32, protocol: u32
) -> Result<Socket> {
	foreach_addr(addr, |addr| {
		let sock = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		sock.connect(addr).await?;

		Ok(sock)
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
		let fd = ops::socket(domain, socket_type, protocol).await?;

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
		ops::close(self.fd).await
	}

	pub async fn connect(&self, addr: &Address) -> Result<()> {
		ops::connect_addr(self.fd(), addr).await
	}

	pub async fn recv(&self, buf: &mut [u8], flags: u32) -> Result<usize> {
		read_into!(buf);

		let read = ops::recv(self.fd(), buf, flags).await?;
		let read = check_interrupt_if_zero(read).await?;

		Ok(read)
	}

	pub async fn recv_vectored(&self, bufs: &mut [IoSliceMut<'_>], flags: u32) -> Result<usize> {
		let mut header = MessageHeader {
			iov: Ptr::from(bufs.as_ptr()).make_mut().cast(),
			iov_len: bufs.len(),

			..Default::default()
		};

		self.recvmsg(&mut header, flags).await
	}

	pub async fn recvmsg(&self, header: &mut MessageHeader, flags: u32) -> Result<usize> {
		let read = ops::recvmsg(self.fd(), header, flags).await?;
		let read = check_interrupt_if_zero(read).await?;

		Ok(read)
	}

	pub async fn send(&self, buf: &[u8], flags: u32) -> Result<usize> {
		write_from!(buf);

		let wrote = ops::send(self.fd(), buf, flags).await?;
		let wrote = check_interrupt_if_zero(wrote).await?;

		Ok(wrote)
	}

	pub async fn sendmsg(&self, header: &MessageHeader, flags: u32) -> Result<usize> {
		let wrote = ops::sendmsg(self.fd(), &header, flags).await?;
		let wrote = check_interrupt_if_zero(wrote).await?;

		Ok(wrote)
	}

	pub async fn send_vectored(&self, bufs: &[IoSlice<'_>], flags: u32) -> Result<usize> {
		let header = MessageHeader {
			iov: Ptr::from(bufs.as_ptr()).make_mut().cast(),
			iov_len: bufs.len(),

			..Default::default()
		};

		self.sendmsg(&header, flags).await
	}

	pub async fn recvfrom(&self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)> {
		let mut addr = AddressStorage::new();
		let mut vec = IoVec {
			base: MutPtr::from(buf.as_mut_ptr()).as_unit(),
			len: buf.len()
		};

		let mut header = MessageHeader {
			address: MutPtr::from(&mut addr).as_unit(),
			address_len: size_of::<AddressStorage>() as u32,

			iov: MutPtr::from(&mut vec),
			iov_len: 1,

			..Default::default()
		};

		let recvd = self.recvmsg(&mut header, flags).await?;

		Ok((recvd, convert_addr(addr)))
	}

	pub async fn sendto(&self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize> {
		write_from!(buf);

		let addr = addr.clone().into();
		let mut vec = IoVec {
			base: Ptr::from(buf.as_ptr()).as_unit().make_mut(),
			len: buf.len()
		};

		let (address, address_len) = {
			match &addr {
				Address::V4(addr) => (
					Ptr::from(addr).make_mut().as_unit(),
					size_of_val(addr) as u32
				),

				Address::V6(addr) => (
					Ptr::from(addr).make_mut().as_unit(),
					size_of_val(addr) as u32
				)
			}
		};

		let header = MessageHeader {
			address,
			address_len,

			iov: MutPtr::from(&mut vec),
			iov_len: 1,

			control: Ptr::null(),
			control_len: 0,

			flags: 0
		};

		self.sendmsg(&header, flags).await
	}

	pub async fn shutdown(&self, how: Shutdown) -> Result<()> {
		ops::shutdown(self.fd(), how).await
	}

	pub async fn set_recvbuf_size(&self, size: i32) -> Result<()> {
		set_recvbuf_size(self.fd(), size)
	}

	pub async fn set_sendbuf_size(&self, size: i32) -> Result<()> {
		set_sendbuf_size(self.fd(), size)
	}

	pub async fn set_tcp_nodelay(&self, enable: bool) -> Result<()> {
		set_tcp_nodelay(self.fd(), enable)
	}

	pub async fn set_tcp_keepalive(&self, enable: bool, idle: i32) -> Result<()> {
		set_tcp_keepalive(self.fd(), enable, idle)
	}

	pub async fn local_addr(&self) -> Result<SocketAddr> {
		let mut addr = AddressStorage::new();

		get_sock_name(self.fd(), &mut addr)?;

		Ok(convert_addr(addr))
	}

	pub async fn peer_addr(&self) -> Result<SocketAddr> {
		let mut addr = AddressStorage::new();

		get_peer_name(self.fd(), &mut addr)?;

		Ok(convert_addr(addr))
	}
}

#[async_trait_impl]
impl Read for Socket {
	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.recv(buf, 0).await
	}

	fn is_read_vectored(&self) -> bool {
		true
	}

	async fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
		self.recv_vectored(bufs, 0).await
	}
}

#[async_trait_impl]
impl Write for Socket {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.send(buf, 0).await
	}

	async fn flush(&mut self) -> Result<()> {
		/* sockets don't need flushing. set nodelay if you want immediate writes */
		Ok(())
	}

	fn is_write_vectored(&self) -> bool {
		true
	}

	async fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<usize> {
		self.send_vectored(bufs, 0).await
	}
}

impl Split for Socket {}

impl From<OwnedFd> for Socket {
	fn from(fd: OwnedFd) -> Self {
		Self { fd }
	}
}

pub struct StreamSocket {
	socket: Socket
}

macro_rules! socket_common {
	() => {
		wrapper_functions! {
			inner = self.socket;

			#[async_fn]
			pub async fn close(self: Self) -> Result<()>;

			#[async_fn]
			pub async fn recv(self: &Self, buf: &mut [u8], flags: u32) -> Result<usize>;

			#[async_fn]
			pub async fn recv_vectored(self: &Self, bufs: &mut [IoSliceMut<'_>], flags: u32) -> Result<usize>;

			#[async_fn]
			pub async fn recvfrom(self: &Self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)>;

			#[async_fn]
			pub async fn recvmsg(&self, header: &mut MessageHeader, flags: u32) -> Result<usize>;

			#[async_fn]
			pub async fn send(self: &Self, buf: &[u8], flags: u32) -> Result<usize>;

			#[async_fn]
			pub async fn send_vectored(self: &Self, bufs: &[IoSlice<'_>], flags: u32) -> Result<usize>;

			#[async_fn]
			pub async fn sendmsg(&self, header: &MessageHeader, flags: u32) -> Result<usize>;

			#[async_fn]
			pub async fn sendto(self: &Self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize>;

			#[async_fn]
			pub async fn shutdown(self: &Self, how: Shutdown) -> Result<()>;

			#[async_fn]
			pub async fn set_recvbuf_size(self: &Self, size: i32) -> Result<()>;

			#[async_fn]
			pub async fn set_sendbuf_size(self: &Self, size: i32) -> Result<()>;

			#[async_fn]
			pub async fn local_addr(&self) -> Result<SocketAddr>;

			#[async_fn]
			pub async fn peer_addr(&self) -> Result<SocketAddr>;
		}
	};
}

macro_rules! socket_impl {
	($type: ty) => {
		impl Read for $type {
			read_wrapper! {
				inner = socket;
				mut inner = socket;
			}
		}

		impl Write for $type {
			write_wrapper! {
				inner = socket;
				mut inner = socket;
			}
		}

		impl Split for $type {}
	};
}

impl StreamSocket {
	socket_common!();

	wrapper_functions! {
		inner = self.socket;

		#[async_fn]
		pub async fn set_tcp_nodelay(self: &Self, enable: bool) -> Result<()>;

		#[async_fn]
		pub async fn set_tcp_keepalive(self: &Self, enable: bool, idle: i32) -> Result<()>;
	}
}

socket_impl!(StreamSocket);

pub struct DatagramSocket {
	socket: Socket
}

impl DatagramSocket {
	socket_common!();

	wrapper_functions! {
		inner = self.socket;

		#[async_fn]
		async fn connect(self: &Self, addr: &Address) -> Result<()>;
	}

	#[async_fn]
	pub async fn connect_addrs<A: ToSocketAddrs>(&self, addrs: A) -> Result<()> {
		foreach_addr(addrs, |addr| self.socket.connect(addr).await).await
	}

	#[async_fn]
	pub async fn recv_from_addr(
		&self, from: &SocketAddr, buf: &mut [u8], flags: u32
	) -> Result<usize> {
		loop {
			let (read, addr) = self.recvfrom(buf, flags).await?;

			if &addr != from {
				continue;
			}

			break Ok(read);
		}
	}
}

socket_impl!(DatagramSocket);

pub struct TcpListener {
	socket: Socket
}

impl TcpListener {
	wrapper_functions! {
		inner = self.socket;

		#[async_fn]
		async fn close(self: Self) -> Result<()>;

		#[async_fn]
		pub async fn local_addr(&self) -> Result<SocketAddr>;

		#[async_fn]
		pub async fn peer_addr(&self) -> Result<SocketAddr>;
	}

	#[async_fn]
	pub async fn accept(&self) -> Result<(StreamSocket, SocketAddr)> {
		let mut storage = AddressStorage::new();
		let (fd, _) = ops::accept(self.socket.fd(), &mut storage).await?;

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
		let sock = connect_addrs(addr, SocketType::Stream as u32, IpProtocol::Tcp as u32).await?;

		Ok(StreamSocket { socket: sock })
	}

	pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<TcpListener> {
		let sock = bind_addr(addr, SocketType::Stream as u32, IpProtocol::Tcp as u32).await?;

		ops::listen(sock.fd(), MAX_BACKLOG).await?;

		Ok(TcpListener { socket: sock })
	}
}

pub struct Udp;

#[async_fn]
impl Udp {
	pub async fn connect<A: ToSocketAddrs>(addrs: A) -> Result<DatagramSocket> {
		let sock =
			connect_addrs(addrs, SocketType::Datagram as u32, IpProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket: sock })
	}

	pub async fn bind<A: ToSocketAddrs>(addrs: A) -> Result<DatagramSocket> {
		let sock = bind_addr(addrs, SocketType::Datagram as u32, IpProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket: sock })
	}
}
