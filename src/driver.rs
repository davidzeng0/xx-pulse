use std::{
	cmp::Ordering,
	collections::BTreeSet,
	ffi::CStr,
	os::fd::{BorrowedFd, OwnedFd},
	time::Duration
};

use enumflags2::{bitflags, BitFlags};
use xx_core::{
	error::*,
	opt::hint::{likely, unlikely},
	os::{
		socket::{MessageHeader, Shutdown},
		stat::Statx,
		time::{self, ClockId}
	},
	pointer::*,
	task::*
};

use super::engine::Engine;

pub struct Driver {
	io_engine: Engine,
	timers: BTreeSet<Timeout>,
	exiting: bool
}

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimeoutFlag {
	Abs = 1 << 0
}

struct Timeout {
	expire: u64,
	request: RequestPtr<Result<()>>
}

impl PartialEq for Timeout {
	fn eq(&self, other: &Self) -> bool {
		self.request == other.request
	}
}

impl Eq for Timeout {}

impl Ord for Timeout {
	fn cmp(&self, other: &Self) -> Ordering {
		let mut ord = self.expire.cmp(&other.expire);

		if ord == Ordering::Equal {
			ord = self.request.cmp(&other.request);
		}

		ord
	}
}

impl PartialOrd for Timeout {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

fn driver_shutdown_error() -> Error {
	Error::new(ErrorKind::Other, "Driver is shutting down")
}

impl Driver {
	pub fn new() -> Result<Self> {
		Ok(Self {
			timers: BTreeSet::new(),
			io_engine: Engine::new()?,
			exiting: false
		})
	}

	#[inline(always)]
	pub fn now() -> u64 {
		time::time(ClockId::Monotonic).expect("Failed to read the clock")
	}

	fn timer_complete(&mut self, timeout: Timeout, result: Result<()>) {
		Request::complete(timeout.request, result);
	}

	/* inline never to prevent compiler from assuming state,
	 * see trait xx_core::task::env::Global
	 */
	#[inline(never)]
	fn expire_first_timer(&mut self, result: Result<()>) {
		let timeout = self.timers.pop_first().unwrap();

		self.timer_complete(timeout, result);
	}

	#[inline(never)]
	pub fn run_timers(&mut self) -> u64 {
		let mut timeout = Duration::from_secs(3600).as_nanos() as u64;
		let mut now = Self::now();
		let mut ran = false;

		loop {
			let timer = match self.timers.first() {
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

			self.expire_first_timer(Ok(()));
		}

		timeout
	}

	fn queue_timer(&mut self, timer: Timeout) {
		self.timers.insert(timer);
	}

	fn cancel_timer(&mut self, timer: Timeout) -> Result<()> {
		match self.timers.take(&timer) {
			None => Err(Error::new(ErrorKind::NotFound, "Timer not found")),
			Some(timeout) => {
				self.timer_complete(
					timeout,
					Err(Error::new(ErrorKind::Interrupted, "Timer cancelled"))
				);

				Ok(())
			}
		}
	}

	#[sync_task]
	#[inline(always)]
	pub fn timeout(&mut self, mut expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
		fn cancel(self: &mut Self, expire: u64) -> Result<()> {
			self.cancel_timer(Timeout { expire, request })
		}

		if unlikely(self.exiting) {
			return Progress::Done(Err(driver_shutdown_error()));
		}

		if !flags.intersects(TimeoutFlag::Abs) {
			expire = expire.saturating_add(Driver::now());
		}

		self.queue_timer(Timeout { expire, request });

		Progress::Pending(cancel(self, expire, request))
	}

	#[inline(always)]
	pub fn park(&mut self, timeout: u64) -> Result<()> {
		self.io_engine.work(timeout)
	}

	#[inline(never)]
	fn expire_all_timers(&mut self) {
		while self.timers.first().is_some() {
			self.expire_first_timer(Err(driver_shutdown_error()));
		}
	}

	pub fn exit(&mut self) -> Result<()> {
		self.exiting = true;
		self.expire_all_timers();

		loop {
			let timeout = self.run_timers();

			if !self.io_engine.has_work() {
				break;
			}

			self.io_engine.work(timeout)?;
		}

		self.exiting = false;

		Ok(())
	}

	pub fn check_exiting(&self) -> Result<()> {
		if likely(!self.exiting) {
			Ok(())
		} else {
			Err(driver_shutdown_error())
		}
	}
}

macro_rules! alias_func {
	($func: ident ($($arg: ident: $type: ty),*)) => {
		#[sync_task]
		pub fn $func(&mut self, $($arg: $type),*) -> isize {
			fn cancel(self: &mut Engine) -> Result<()> {
				/* use this fn to generate the cancel closure type */
				Ok(())
			}

			let task = self.io_engine.$func($($arg),*);

			unsafe {
				task.run(request)
			}
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

	alias_func!(connect(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32));

	alias_func!(recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32));

	alias_func!(recvmsg(socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32));

	alias_func!(send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32));

	alias_func!(sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32));

	alias_func!(shutdown(socket: BorrowedFd<'_>, how: Shutdown));

	alias_func!(bind(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32));

	alias_func!(listen(socket: BorrowedFd<'_>, backlog: i32));

	alias_func!(fsync(file: BorrowedFd<'_>));

	alias_func!(statx(path: &CStr, flags: u32, mask: u32, statx: &mut Statx));

	alias_func!(poll(fd: BorrowedFd<'_>, mask: u32));
}

impl Global for Driver {}
