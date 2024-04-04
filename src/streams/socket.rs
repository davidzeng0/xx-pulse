#![allow(clippy::module_name_repetitions)]

use std::net::{SocketAddr, ToSocketAddrs};

use xx_core::{
	async_std::io::*,
	coroutines::acquire_budget,
	impls::{AsyncFn, AsyncFnOnce},
	macros::*,
	os::{epoll::PollFlag, error::OsError, inet::*, socket::*}
};

use super::*;

#[asynchronous]
async fn foreach_addr<A, F, Output>(addrs: A, f: F) -> Result<Output>
where
	A: ToSocketAddrs,
	F: AsyncFn<Address, Output = Result<Output>>
{
	let mut error = None;

	for addr in addrs.to_socket_addrs()? {
		match f.call(addr.into()).await {
			Ok(out) => return Ok(out),
			Err(err) => error = Some(err)
		}
	}

	Err(error.unwrap_or_else(|| Core::NoAddresses.into()))
}

#[asynchronous]
async fn bind_addr<A>(addr: A, socket_type: u32, protocol: IpProtocol) -> Result<Socket>
where
	A: ToSocketAddrs
{
	foreach_addr(addr, |addr| async move {
		let sock = Socket::new_for_addr(&addr, socket_type, protocol).await?;

		set_reuse_addr(sock.fd(), true)?;
		io::bind_addr(sock.fd(), &addr).await?;

		Ok(sock)
	})
	.await
}

#[asynchronous]
async fn connect_addrs<A>(addr: A, socket_type: u32, protocol: IpProtocol) -> Result<Socket>
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
	fd: OwnedFd,
	ready: BitFlags<PollFlag>
}

#[asynchronous]
async fn with_budget<T, U, Sync, Suspend>(
	socket: &mut Socket, mut data: T, flags: BitFlags<PollFlag>, sync: Sync, suspend: Suspend
) -> Result<U>
where
	Sync: FnOnce(BorrowedFd<'_>, &mut T) -> OsResult<U>,
	Suspend: for<'a, 'b> AsyncFnOnce<(BorrowedFd<'a>, &'b mut T), Output = Result<U>>
{
	if socket.ready.contains(flags) && acquire_budget(None).await {
		match sync(socket.fd(), &mut data) {
			Ok(result) => return Ok(result),
			Err(OsError::WouldBlock) => socket.ready.remove(flags),
			Err(err) => return Err(err.into())
		}
	}

	let result = suspend.call_once((socket.fd(), &mut data)).await;

	if result.is_ok() {
		socket.ready.insert(flags);
	}

	result
}

#[asynchronous]
impl Socket {
	pub async fn new(
		domain: AddressFamily, socket_type: u32, protocol: IpProtocol
	) -> Result<Self> {
		let fd = io::socket(domain, socket_type, protocol).await?;

		Ok(Self { fd, ready: BitFlags::default() })
	}

	pub async fn new_for_addr(
		addr: &Address, socket_type: u32, protocol: IpProtocol
	) -> Result<Self> {
		match addr {
			Address::V4(_) => Self::new(AddressFamily::INet, socket_type, protocol),
			Address::V6(_) => Self::new(AddressFamily::INet6, socket_type, protocol)
		}
		.await
	}

	#[must_use]
	pub fn fd(&self) -> BorrowedFd<'_> {
		self.fd.as_fd()
	}

	pub async fn close(self) -> Result<()> {
		io::close(self.fd).await
	}

	pub async fn connect(&self, addr: &Address) -> Result<()> {
		io::connect_addr(self.fd(), addr).await
	}

	pub async fn recv(&mut self, buf: &mut [u8], flags: BitFlags<MessageFlag>) -> Result<usize> {
		read_into!(buf);

		with_budget(
			self,
			buf,
			PollFlag::In.into(),
			|fd: BorrowedFd<'_>, buf: &mut &mut [u8]| {
				/* Safety: buf is valid */
				unsafe { recv(fd, (*buf).into(), flags | MessageFlag::DontWait) }
			},
			|fd: BorrowedFd<'_>, buf: &mut &mut [u8]| async move {
				check_interrupt_if_zero(io::recv(fd, buf, flags).await?).await
			}
		)
		.await
	}

	pub async fn recv_vectored(
		&mut self, bufs: &mut [IoSliceMut<'_>], flags: BitFlags<MessageFlag>
	) -> Result<usize> {
		let mut header = MsgHdrMut::default();

		header.set_vecs(IoVecMut::from_io_slices_mut(bufs));

		self.recvmsg(&mut header, flags).await
	}

	pub async fn recvmsg(
		&mut self, header: &mut MsgHdrMut<'_>, flags: BitFlags<MessageFlag>
	) -> Result<usize> {
		with_budget(
			self,
			header,
			PollFlag::In.into(),
			|fd: BorrowedFd<'_>, buf: &mut &mut MsgHdrMut<'_>| {
				recvmsg(fd, buf, flags | MessageFlag::DontWait)
			},
			|fd: BorrowedFd<'_>, buf: &mut &mut MsgHdrMut<'_>| async move {
				check_interrupt_if_zero(io::recvmsg(fd, buf, flags).await?).await
			}
		)
		.await
	}

	pub async fn send(&mut self, buf: &[u8], flags: BitFlags<MessageFlag>) -> Result<usize> {
		write_from!(buf);

		with_budget(
			self,
			buf,
			PollFlag::Out.into(),
			|fd: BorrowedFd<'_>, buf: &mut &[u8]| {
				/* Safety: buf is valid */
				unsafe { send(fd, (*buf).into(), flags | MessageFlag::DontWait) }
			},
			|fd: BorrowedFd<'_>, buf: &mut &[u8]| async move {
				check_interrupt_if_zero(io::send(fd, buf, flags).await?).await
			}
		)
		.await
	}

	pub async fn sendmsg(
		&mut self, header: &MsgHdr<'_>, flags: BitFlags<MessageFlag>
	) -> Result<usize> {
		with_budget(
			self,
			header,
			PollFlag::Out.into(),
			|fd: BorrowedFd<'_>, buf: &mut &MsgHdr<'_>| {
				sendmsg(fd, buf, flags | MessageFlag::DontWait)
			},
			|fd: BorrowedFd<'_>, buf: &mut &MsgHdr<'_>| async move {
				check_interrupt_if_zero(io::sendmsg(fd, buf, flags).await?).await
			}
		)
		.await
	}

	pub async fn send_vectored(
		&mut self, bufs: &[IoSlice<'_>], flags: BitFlags<MessageFlag>
	) -> Result<usize> {
		let mut header = MsgHdr::default();

		header.set_vecs(IoVec::from_io_slices(bufs));

		self.sendmsg(&header, flags).await
	}

	pub async fn recvfrom(
		&mut self, buf: &mut [u8], flags: BitFlags<MessageFlag>
	) -> Result<(usize, SocketAddr)> {
		let mut addr = AddressStorage::default();
		let mut vecs = [IoVecMut::from(buf)];
		let mut header = MsgHdrMut::default();

		header.set_addr(&mut addr);
		header.set_vecs(&mut vecs[..]);

		let recvd = self.recvmsg(&mut header, flags).await?;

		Ok((recvd, convert_addr(addr)))
	}

	pub async fn sendto(
		&mut self, buf: &[u8], flags: BitFlags<MessageFlag>, addr: &SocketAddr
	) -> Result<usize> {
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

	pub async fn poll(&mut self, flags: BitFlags<PollFlag>) -> Result<BitFlags<PollFlag>> {
		let result = io::poll(self.fd(), flags).await?;
		let flags = PollFlag::In | PollFlag::Out;

		self.ready.remove(flags);
		self.ready.insert(result & flags);

		Ok(flags)
	}

	pub async fn shutdown(&mut self, how: Shutdown) -> Result<()> {
		io::shutdown(self.fd(), how).await?;

		self.ready.insert(PollFlag::In | PollFlag::Out);

		Ok(())
	}

	pub async fn set_recvbuf_size(&self, size: i32) -> Result<()> {
		set_recvbuf_size(self.fd(), size).map_err(Into::into)
	}

	pub async fn set_sendbuf_size(&self, size: i32) -> Result<()> {
		set_sendbuf_size(self.fd(), size).map_err(Into::into)
	}

	pub async fn set_tcp_nodelay(&self, enable: bool) -> Result<()> {
		set_tcp_nodelay(self.fd(), enable).map_err(Into::into)
	}

	pub async fn set_tcp_keepalive(&self, enable: bool, idle: i32) -> Result<()> {
		set_tcp_keepalive(self.fd(), enable, idle).map_err(Into::into)
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
		self.recv(buf, BitFlags::default()).await
	}

	fn is_read_vectored(&self) -> bool {
		true
	}

	async fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
		self.recv_vectored(bufs, BitFlags::default()).await
	}
}

#[asynchronous]
impl Write for Socket {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.send(buf, BitFlags::default()).await
	}

	async fn flush(&mut self) -> Result<()> {
		/* sockets don't need flushing. set nodelay if you want immediate writes */
		Ok(())
	}

	fn is_write_vectored(&self) -> bool {
		true
	}

	async fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<usize> {
		self.send_vectored(bufs, BitFlags::default()).await
	}
}

impl From<OwnedFd> for Socket {
	fn from(fd: OwnedFd) -> Self {
		Self { fd, ready: BitFlags::default() }
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
			pub async fn recv(&mut self, buf: &mut [u8], flags: BitFlags<MessageFlag>) -> Result<usize>;

			#[asynchronous]
			pub async fn recv_vectored(&mut self, bufs: &mut [IoSliceMut<'_>], flags: BitFlags<MessageFlag>) -> Result<usize>;

			#[asynchronous]
			pub async fn recvfrom(&mut self, buf: &mut [u8], flags: BitFlags<MessageFlag>) -> Result<(usize, SocketAddr)>;

			#[asynchronous]
			pub async fn recvmsg(&mut self, header: &mut MsgHdrMut<'_>, flags: BitFlags<MessageFlag>) -> Result<usize>;

			#[asynchronous]
			pub async fn send(&mut self, buf: &[u8], flags: BitFlags<MessageFlag>) -> Result<usize>;

			#[asynchronous]
			pub async fn send_vectored(&mut self, bufs: &[IoSlice<'_>], flags: BitFlags<MessageFlag>) -> Result<usize>;

			#[asynchronous]
			pub async fn sendmsg(&mut self, header: &MsgHdr<'_>, flags: BitFlags<MessageFlag>) -> Result<usize>;

			#[asynchronous]
			pub async fn sendto(&mut self, buf: &[u8], flags: BitFlags<MessageFlag>, addr: &SocketAddr) -> Result<usize>;

			#[asynchronous]
			pub async fn poll(&mut self, flags: BitFlags<PollFlag>) -> Result<BitFlags<PollFlag>>;

			#[asynchronous]
			pub async fn shutdown(&mut self, how: Shutdown) -> Result<()>;

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
		&mut self, from: &SocketAddr, buf: &mut [u8], flags: BitFlags<MessageFlag>
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
		let (fd, _) = io::accept(self.socket.fd(), &mut storage).await?;

		Ok((StreamSocket { socket: fd.into() }, convert_addr(storage)))
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
		let sock = connect_addrs(addr, SocketType::Stream as u32, IpProtocol::Tcp).await?;

		Ok(StreamSocket { socket: sock })
	}

	pub async fn bind<A>(addr: A) -> Result<TcpListener>
	where
		A: ToSocketAddrs
	{
		let sock = bind_addr(addr, SocketType::Stream as u32, IpProtocol::Tcp).await?;

		io::listen(sock.fd(), MAX_BACKLOG).await?;

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
		let sock = connect_addrs(addrs, SocketType::Datagram as u32, IpProtocol::Udp).await?;

		Ok(DatagramSocket { socket: sock })
	}

	pub async fn bind<A>(addrs: A) -> Result<DatagramSocket>
	where
		A: ToSocketAddrs
	{
		let sock = bind_addr(addrs, SocketType::Datagram as u32, IpProtocol::Udp).await?;

		Ok(DatagramSocket { socket: sock })
	}
}
