#![allow(unreachable_pub)]

use std::{collections::BTreeSet, os::fd::RawFd};

use enumflags2::*;
use xx_core::{
	macros::{duration, panic_nounwind},
	opt::hint::*,
	os::{
		socket::raw,
		stat::Statx,
		time::{self, ClockId}
	},
	pointer::*
};

use super::*;

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimeoutFlag {
	Abs = 1 << 0
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Timeout {
	expire: u64,
	request: ReqPtr<Result<()>>
}

#[allow(clippy::module_name_repetitions, missing_copy_implementations)]
#[errors]
pub enum DriverError {
	#[error("Driver is shutting down")]
	Shutdown,

	#[error("Timer not found")]
	TimerNotFound
}

struct DriverInner {
	timers: BTreeSet<Timeout>,
	exiting: bool
}

pub struct Driver {
	inner: UnsafeCell<DriverInner>,
	io_engine: Engine
}

impl Driver {
	pub fn new() -> Result<Self> {
		Ok(Self {
			inner: UnsafeCell::new(DriverInner { timers: BTreeSet::new(), exiting: false }),
			io_engine: Engine::new()?
		})
	}

	#[inline(always)]
	fn try_time() -> Result<u64> {
		time::time(ClockId::Monotonic)
	}

	#[inline(always)]
	fn time_or_abort() -> u64 {
		match Self::try_time() {
			Ok(time) => time,
			Err(err) => panic_nounwind!("Failed to read the clock: {:?}", err)
		}
	}

	#[allow(clippy::expect_used)]
	pub fn time() -> u64 {
		Self::try_time().expect("Failed to read the clock")
	}

	unsafe fn timer_complete(timeout: Timeout, result: Result<()>) {
		/* Safety: guaranteed by caller */
		unsafe { Request::complete(timeout.request, result) };
	}

	fn expire_first_timer(timers: &mut BTreeSet<Timeout>, result: Result<()>) {
		#[allow(clippy::unwrap_used)]
		let timeout = timers.pop_first().unwrap();

		/* Safety: complete the future */
		unsafe { Self::timer_complete(timeout, result) };
	}

	fn queue_timer(&self, timer: Timeout) {
		/* Safety: exclusive unsafe cell access */
		unsafe { ptr!(self.inner=>timers.insert(timer)) };
	}

	fn cancel_timer(&self, timer: Timeout) -> Result<()> {
		/* Safety: we have exclusive mutable access until expire */
		let timeout = match unsafe { ptr!(self.inner=>timers.take(&timer)) } {
			Some(timeout) => timeout,
			None => return Err(DriverError::TimerNotFound.into())
		};

		#[cfg(feature = "tracing-ext")]
		xx_core::trace!(target: self, "## cancel_timer(request = {:?}) = Ok(reason = cancel)", timeout.request);

		/* Safety: complete the future */
		unsafe { Self::timer_complete(timeout, Err(Core::Interrupted("Timer cancelled").into())) };

		Ok(())
	}

	#[future]
	pub fn timeout(&self, mut expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
		#[cancel]
		fn cancel(&self, expire: u64) -> Result<()> {
			self.cancel_timer(Timeout { expire, request })
		}

		if let Err(err) = self.check_exiting() {
			return Progress::Done(Err(err));
		}

		if !flags.intersects(TimeoutFlag::Abs) {
			expire = expire.saturating_add(Self::time());
		}

		#[cfg(feature = "tracing-ext")]
		xx_core::trace!(target: self, "## timeout(expire = {}, request = {:?}) = Ok(())", expire, request);

		self.queue_timer(Timeout { expire, request });

		Progress::Pending(cancel(self, expire, request))
	}

	#[inline(always)]
	pub fn run_timers(&self) -> u64 {
		#[allow(clippy::cast_possible_truncation)]
		let mut timeout = duration!(1 h).as_nanos() as u64;
		let mut ran = false;
		let mut now = Self::time_or_abort();

		loop {
			/* Safety: we have mutable access until expire */
			let timers = unsafe { &mut ptr!(self.inner=>timers) };
			let timer = match timers.first() {
				None => break,
				Some(timer) => timer
			};

			if timer.expire > now {
				if ran {
					now = Self::time_or_abort();
				}

				timeout = timer.expire.saturating_sub(now);

				break;
			}

			ran = true;

			#[cfg(feature = "tracing-ext")]
			xx_core::trace!(target: self, "## run_timers: complete(request = {:?}, reason = timeout)", timer.request);

			Self::expire_first_timer(timers, Ok(()));
		}

		timeout
	}

	#[inline(always)]
	pub fn park(&self, timeout: u64) {
		match self.io_engine.work(timeout) {
			Ok(()) => (),
			Err(err) => panic_nounwind!("Fatal error from engine: {:?}", err)
		}
	}

	// FIXME: force all fibers to exit when the driver exits, or use-after-free
	// could occur
	pub fn exit(&self) {
		/* Safety: exclusive unsafe cell access */
		unsafe { ptr!(self.inner=>exiting = true) };

		loop {
			/* Safety: we have exclusive access until expire */
			let timers = unsafe { &mut ptr!(self.inner=>timers) };

			if timers.is_empty() {
				break;
			}

			Self::expire_first_timer(timers, Err(DriverError::Shutdown.into()));
		}

		loop {
			let timeout = self.run_timers();

			if !self.io_engine.has_work() {
				break;
			}

			self.park(timeout);
		}

		/* Safety: exclusive unsafe cell access */
		unsafe { ptr!(self.inner=>exiting = false) };
	}

	#[inline(always)]
	pub fn check_exiting(&self) -> Result<()> {
		/* Safety: exclusive unsafe cell access */
		if likely(!unsafe { self.inner.as_ref().exiting }) {
			Ok(())
		} else {
			Err(DriverError::Shutdown.into())
		}
	}
}

macro_rules! engine_task {
	($func: ident ($($arg: ident: $type: ty),*)) => {
		#[future]
		pub unsafe fn $func(&self, $($arg: $type),*) -> isize {
			#[cancel]
			fn cancel(engine: &Engine) -> Result<()> {
				/* use this fn to generate the cancel closure type */
				Ok(())
			}

			#[allow(clippy::multiple_unsafe_ops_per_block)]
			/* Safety: guaranteed by caller */
			unsafe { self.io_engine.$func($($arg),*).run(request) }
		}
	}
}

impl Driver {
	engine_task!(open(path: Ptr<()>, flags: u32, mode: u32));

	engine_task!(close(fd: RawFd));

	engine_task!(read(fd: RawFd, buf: MutPtr<()>, len: usize, offset: i64));

	engine_task!(write(fd: RawFd, buf: Ptr<()>, len: usize, offset: i64));

	engine_task!(socket(domain: u32, sockettype: u32, protocol: u32));

	engine_task!(accept(socket: RawFd, addr: MutPtr<()>, addrlen: MutPtr<i32>));

	engine_task!(connect(socket: RawFd, addr: Ptr<()>, addrlen: i32));

	engine_task!(recv(socket: RawFd, buf: MutPtr<()>, len: usize, flags: u32));

	engine_task!(recvmsg(socket: RawFd, header: MutPtr<raw::MsgHdr>, flags: u32));

	engine_task!(send(socket: RawFd, buf: Ptr<()>, len: usize, flags: u32));

	engine_task!(sendmsg(socket: RawFd, header: Ptr<raw::MsgHdr>, flags: u32));

	engine_task!(shutdown(socket: RawFd, how: u32));

	engine_task!(bind(socket: RawFd, addr: Ptr<()>, addrlen: i32));

	engine_task!(listen(socket: RawFd, backlog: i32));

	engine_task!(fsync(file: RawFd));

	engine_task!(statx(dirfd: RawFd, path: Ptr<()>, flags: u32, mask: u32, statx: MutPtr<Statx>));

	engine_task!(poll(fd: RawFd, mask: u32));
}
