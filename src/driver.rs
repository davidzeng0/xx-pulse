use std::{
	collections::BTreeSet,
	ffi::CStr,
	os::fd::{BorrowedFd, OwnedFd}
};

use enumflags2::*;
use xx_core::{
	error::*,
	future::*,
	macros::duration,
	opt::hint::*,
	os::{
		socket::{MessageHeader, MessageHeaderMut, Shutdown},
		stat::Statx,
		time::{self, ClockId}
	},
	pointer::*
};

use super::engine::Engine;

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimeoutFlag {
	Abs = 1 << 0
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Timeout {
	expire: u64,
	request: ReqPtr<Result<()>>
}

#[compact_error]
pub enum DriverError {
	Shutdown       = (ErrorKind::Other, "Driver is shutting down"),
	TimerCancelled = (ErrorKind::Interrupted, "Timer cancelled"),
	TimerNotFound  = (ErrorKind::NotFound, "Timer not found")
}

struct DriverInner {
	io_engine: Engine,
	timers: BTreeSet<Timeout>,
	exiting: bool
}

pub struct Driver {
	inner: UnsafeCell<DriverInner>
}

impl Driver {
	pub fn new() -> Result<Self> {
		Ok(Self {
			inner: UnsafeCell::new(DriverInner {
				timers: BTreeSet::new(),
				io_engine: Engine::new()?,
				exiting: false
			})
		})
	}

	#[inline(always)]
	pub fn now() -> u64 {
		time::time(ClockId::Monotonic).expect("Failed to read the clock")
	}

	#[inline(always)]
	fn timer_complete(timeout: Timeout, result: Result<()>) {
		unsafe { Request::complete(timeout.request, result) };
	}

	#[inline(always)]
	fn expire_first_timer(timers: &mut BTreeSet<Timeout>, result: Result<()>) {
		let timeout = timers.pop_first().unwrap();

		Self::timer_complete(timeout, result);
	}

	fn queue_timer(&self, timer: Timeout) {
		unsafe { self.inner.as_mut().timers.insert(timer) };
	}

	fn cancel_timer(&self, timer: Timeout) -> Result<()> {
		let timeout = match unsafe { self.inner.as_mut().timers.take(&timer) } {
			Some(timeout) => timeout,
			None => return Err(DriverError::TimerNotFound.new())
		};

		Self::timer_complete(timeout, Err(DriverError::TimerCancelled.new()));

		Ok(())
	}

	#[future]
	pub fn timeout(&self, mut expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
		#[cancel]
		fn cancel(self: &Self, expire: u64) -> Result<()> {
			self.cancel_timer(Timeout { expire, request })
		}

		if unlikely(unsafe { self.inner.as_ref().exiting }) {
			return Progress::Done(Err(DriverError::Shutdown.new()));
		}

		if !flags.intersects(TimeoutFlag::Abs) {
			expire = expire.saturating_add(Driver::now());
		}

		self.queue_timer(Timeout { expire, request });

		Progress::Pending(cancel(self, expire, request))
	}

	#[inline(always)]
	pub fn run_timers(&self) -> u64 {
		let mut timeout = duration!(1 h).as_nanos() as u64;
		let mut now = Self::now();
		let mut ran = false;

		loop {
			let timers = unsafe { &mut self.inner.as_mut().timers };
			let timer = match timers.first() {
				None => break,
				Some(timer) => timer
			};

			if timer.expire > now {
				if ran {
					now = Self::now();
				}

				timeout = timer.expire.saturating_sub(now);

				break;
			}

			ran = true;

			Self::expire_first_timer(timers, Ok(()));
		}

		timeout
	}

	#[inline(always)]
	pub fn park(&self, timeout: u64) -> Result<()> {
		unsafe { self.inner.as_mut().io_engine.work(timeout) }
	}

	pub fn exit(&self) -> Result<()> {
		unsafe {
			let exiting = { &mut self.inner.as_mut().exiting };

			*exiting = true;

			loop {
				let timers = &mut self.inner.as_mut().timers;

				if timers.is_empty() {
					break;
				}

				Self::expire_first_timer(timers, Err(DriverError::Shutdown.new()))
			}

			loop {
				let timeout = self.run_timers();
				let engine = &mut self.inner.as_mut().io_engine;

				if !engine.has_work() {
					break;
				}

				engine.work(timeout)?;
			}

			*exiting = false;
		}

		Ok(())
	}

	#[inline(always)]
	pub fn check_exiting(&self) -> Result<()> {
		if likely(!unsafe { self.inner.as_ref().exiting }) {
			Ok(())
		} else {
			Err(DriverError::Shutdown.new())
		}
	}
}

macro_rules! alias_func {
	($func: ident ($($arg: ident: $type: ty),*)) => {
		#[future]
		pub unsafe fn $func(&self, $($arg: $type),*) -> isize {
			#[cancel]
			fn cancel(self: &mut Engine) -> Result<()> {
				/* use this fn to generate the cancel closure type */
				Ok(())
			}

			self.inner.as_mut().io_engine.$func($($arg),*).run(request)
		}
	}
}

impl Driver {
	alias_func!(open(path: &CStr, flags: u32, mode: u32));

	alias_func!(close(fd: OwnedFd));

	alias_func!(read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64));

	alias_func!(write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64));

	alias_func!(socket(domain: u32, socket_type: u32, protocol: u32));

	alias_func!(accept(socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32));

	alias_func!(connect(socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32));

	alias_func!(recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32));

	alias_func!(recvmsg(socket: BorrowedFd<'_>, header: &mut MessageHeaderMut<'_>, flags: u32));

	alias_func!(send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32));

	alias_func!(sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader<'_>, flags: u32));

	alias_func!(shutdown(socket: BorrowedFd<'_>, how: Shutdown));

	alias_func!(bind(socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32));

	alias_func!(listen(socket: BorrowedFd<'_>, backlog: i32));

	alias_func!(fsync(file: BorrowedFd<'_>));

	alias_func!(statx(path: &CStr, flags: u32, mask: u32, statx: &mut Statx));

	alias_func!(poll(fd: BorrowedFd<'_>, mask: u32));
}
