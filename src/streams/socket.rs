#![allow(clippy::module_name_repetitions)]

use std::net::{SocketAddr, ToSocketAddrs};

use xx_core::{
	impls::AsyncFn1,
	macros::wrapper_functions,
	os::{inet::*, poll::PollFlag, socket::*},
	read_wrapper, write_wrapper
};

use super::*;

#[asynchronous]
async fn foreach_addr<A: ToSocketAddrs, F: AsyncFn1<Address, Output = Result<Output>>, Output>(
	addrs: A, f: F
) -> Result<Output> {
	let mut error = None;

	for addr in addrs.to_socket_addrs()? {
		match f(addr.into()).await {
			Ok(out) => return Ok(out),
			Err(err) => error = Some(err)
		}
	}

	Err(error.unwrap_or_else(|| Core::NoAddresses.as_err()))
}

#[asynchronous]
async fn bind_addr<A>(addr: A, socket_type: u32, protocol: u32) -> Result<Socket>
where
	A: ToSocketAddrs
{
	foreach_addr(addr, |addr| async move {
		let sock = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		set_reuse_addr(sock.fd(), true)?;
		ops::bind_addr(sock.fd(), &addr).await?;

		Ok(sock)
	})
	.await
}

#[asynchronous]
async fn connect_addrs<A>(addr: A, socket_type: u32, protocol: u32) -> Result<Socket>
where
	A: ToSocketAddrs
{
	foreach_addr(addr, |addr| async move {
		let sock = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		sock.connect(&addr).await?;

		Ok(sock)
	})
	.await
}

#[allow(clippy::unwrap_used)]
fn convert_addr(storage: AddressStorage) -> SocketAddr {
	/* into should be safe here unless OS is broken */
	storage.try_into().unwrap()
}

pub struct Socket {
	fd: OwnedFd
}

#[asynchronous]
impl Socket {
	pub async fn new(domain: u32, socket_type: u32, protocol: u32) -> Result<Self> {
		let fd = ops::socket(domain, socket_type, protocol).await?;

		Ok(Self { fd })
	}

	pub async fn new_for_addr(addr: &Address, socket_type: u32, protocol: u32) -> Result<Self> {
		match addr {
			Address::V4(_) => Self::new(AddressFamily::INet as u32, socket_type, protocol),
			Address::V6(_) => Self::new(AddressFamily::INet6 as u32, socket_type, protocol)
		}
		.await
	}

	#[must_use]
	pub fn fd(&self) -> BorrowedFd<'_> {
		self.fd.as_fd()
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}

	pub async fn connect(&self, addr: &Address) -> Result<()> {
		ops::connect_addr(self.fd(), addr).await
	}

	pub async fn recv(&self, buf: &mut [u8], flags: u32) -> Result<usize> {
		read_into!(buf);

		let read = ops::recv(self.fd(), buf, flags).await?;

		check_interrupt_if_zero(read).await
	}

	pub async fn recv_vectored(&self, bufs: &mut [IoSliceMut<'_>], flags: u32) -> Result<usize> {
		let mut header = MsgHdrMut::default();

		header.set_vecs(IoVecMut::from_io_slices_mut(bufs));

		self.recvmsg(&mut header, flags).await
	}

	pub async fn recvmsg(&self, header: &mut MsgHdrMut<'_>, flags: u32) -> Result<usize> {
		let read = recvmsg(self.fd(), header, flags).await?;

		check_interrupt_if_zero(read).await
	}

	pub async fn send(&self, buf: &[u8], flags: u32) -> Result<usize> {
		write_from!(buf);

		let wrote = ops::send(self.fd(), buf, flags).await?;

		check_interrupt_if_zero(wrote).await
	}

	pub async fn sendmsg(&self, header: &MsgHdr<'_>, flags: u32) -> Result<usize> {
		let wrote = sendmsg(self.fd(), header, flags).await?;

		check_interrupt_if_zero(wrote).await
	}

	pub async fn send_vectored(&self, bufs: &[IoSlice<'_>], flags: u32) -> Result<usize> {
		let mut header = MsgHdr::default();

		header.set_vecs(IoVec::from_io_slices(bufs));

		self.sendmsg(&header, flags).await
	}

	pub async fn recvfrom(&self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)> {
		let mut addr = AddressStorage::default();
		let mut vecs = [IoVecMut::from(buf)];
		let mut header = MsgHdrMut::default();

		header.set_addr(&mut addr);
		header.set_vecs(&mut vecs[..]);

		let recvd = self.recvmsg(&mut header, flags).await?;

		Ok((recvd, convert_addr(addr)))
	}

	pub async fn sendto(&self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize> {
		write_from!(buf);

		let mut header = MsgHdr::default();
		let vecs = [IoVec::from(buf)];
		let addr = (*addr).into();

		match &addr {
			Address::V4(addr) => header.set_addr(addr),
			Address::V6(addr) => header.set_addr(addr)
		}

		header.set_vecs(&vecs[..]);

		self.sendmsg(&header, flags).await
	}

	pub async fn poll(&self, flags: BitFlags<PollFlag>) -> Result<BitFlags<PollFlag>> {
		let bits = poll(self.fd(), flags.bits()).await?;

		Ok(BitFlags::from_bits_truncate(bits))
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
		let mut addr = AddressStorage::default();

		get_sock_name(self.fd(), &mut addr)?;

		Ok(convert_addr(addr))
	}

	pub async fn peer_addr(&self) -> Result<SocketAddr> {
		let mut addr = AddressStorage::default();

		get_peer_name(self.fd(), &mut addr)?;

		Ok(convert_addr(addr))
	}
}

#[asynchronous]
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

#[asynchronous]
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

/* Safety: sockets don't have any mutable state in rust land */
unsafe impl SimpleSplit for Socket {}

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

			#[asynchronous]
			pub async fn close(self) -> Result<()>;

			#[asynchronous]
			pub async fn recv(&self, buf: &mut [u8], flags: u32) -> Result<usize>;

			#[asynchronous]
			pub async fn recv_vectored(&self, bufs: &mut [IoSliceMut<'_>], flags: u32) -> Result<usize>;

			#[asynchronous]
			pub async fn recvfrom(&self, buf: &mut [u8], flags: u32) -> Result<(usize, SocketAddr)>;

			#[asynchronous]
			pub async fn recvmsg(&self, header: &mut MsgHdrMut<'_>, flags: u32) -> Result<usize>;

			#[asynchronous]
			pub async fn send(&self, buf: &[u8], flags: u32) -> Result<usize>;

			#[asynchronous]
			pub async fn send_vectored(&self, bufs: &[IoSlice<'_>], flags: u32) -> Result<usize>;

			#[asynchronous]
			pub async fn sendmsg(&self, header: &MsgHdr<'_>, flags: u32) -> Result<usize>;

			#[asynchronous]
			pub async fn sendto(&self, buf: &[u8], flags: u32, addr: &SocketAddr) -> Result<usize>;

			#[asynchronous]
			pub async fn poll(&self, flags: BitFlags<PollFlag>) -> Result<BitFlags<PollFlag>>;

			#[asynchronous]
			pub async fn shutdown(&self, how: Shutdown) -> Result<()>;

			#[asynchronous]
			pub async fn set_recvbuf_size(&self, size: i32) -> Result<()>;

			#[asynchronous]
			pub async fn set_sendbuf_size(&self, size: i32) -> Result<()>;

			#[asynchronous]
			pub async fn local_addr(&self) -> Result<SocketAddr>;

			#[asynchronous]
			pub async fn peer_addr(&self) -> Result<SocketAddr>;
		}
	};
}

macro_rules! socket_impl {
	($type:ty) => {
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

		/* Safety: sockets don't have any mutable state in rust land */
		unsafe impl SimpleSplit for $type {}
	};
}

impl StreamSocket {
	socket_common!();

	wrapper_functions! {
		inner = self.socket;

		#[asynchronous]
		pub async fn set_tcp_nodelay(&self, enable: bool) -> Result<()>;

		#[asynchronous]
		pub async fn set_tcp_keepalive(&self, enable: bool, idle: i32) -> Result<()>;
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

		#[asynchronous]
		async fn connect(&self, addr: &Address) -> Result<()>;
	}

	#[asynchronous]
	pub async fn connect_addrs<A>(&self, addrs: A) -> Result<()>
	where
		A: ToSocketAddrs
	{
		foreach_addr(
			addrs,
			|addr| async move { self.socket.connect(&addr).await }
		)
		.await
	}

	#[asynchronous]
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

		#[asynchronous]
		async fn close(self) -> Result<()>;

		#[asynchronous]
		pub async fn local_addr(&self) -> Result<SocketAddr>;

		#[asynchronous]
		pub async fn peer_addr(&self) -> Result<SocketAddr>;
	}

	#[asynchronous]
	pub async fn accept(&self) -> Result<(StreamSocket, SocketAddr)> {
		let mut storage = AddressStorage::default();
		let (fd, _) = ops::accept(self.socket.fd(), &mut storage).await?;

		Ok((
			StreamSocket { socket: Socket { fd } },
			convert_addr(storage)
		))
	}
}

#[allow(missing_copy_implementations)]
pub struct Tcp;

#[asynchronous]
impl Tcp {
	pub async fn connect<A>(addr: A) -> Result<StreamSocket>
	where
		A: ToSocketAddrs
	{
		let sock = connect_addrs(addr, SocketType::Stream as u32, IpProtocol::Tcp as u32).await?;

		Ok(StreamSocket { socket: sock })
	}

	pub async fn bind<A>(addr: A) -> Result<TcpListener>
	where
		A: ToSocketAddrs
	{
		let sock = bind_addr(addr, SocketType::Stream as u32, IpProtocol::Tcp as u32).await?;

		ops::listen(sock.fd(), MAX_BACKLOG).await?;

		Ok(TcpListener { socket: sock })
	}
}

#[allow(missing_copy_implementations)]
pub struct Udp;

#[asynchronous]
impl Udp {
	pub async fn connect<A>(addrs: A) -> Result<DatagramSocket>
	where
		A: ToSocketAddrs
	{
		let sock =
			connect_addrs(addrs, SocketType::Datagram as u32, IpProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket: sock })
	}

	pub async fn bind<A>(addrs: A) -> Result<DatagramSocket>
	where
		A: ToSocketAddrs
	{
		let sock = bind_addr(addrs, SocketType::Datagram as u32, IpProtocol::Udp as u32).await?;

		Ok(DatagramSocket { socket: sock })
	}
}
