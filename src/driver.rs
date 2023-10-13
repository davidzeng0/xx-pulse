use std::{
	cmp::Ordering,
	collections::BTreeSet,
	ffi::CStr,
	io::{Error, ErrorKind, Result},
	os::fd::{BorrowedFd, OwnedFd},
	time::Duration
};

use enumflags2::{bitflags, BitFlags};
use xx_core::{
	os::{
		socket::{MessageHeader, Shutdown},
		time::{self, ClockId}
	},
	pointer::{ConstPtr, MutPtr},
	task::{env::Global, sync_task, Progress, Request, RequestPtr}
};

use super::engine::{Engine, EngineImpl};

macro_rules! engine_task {
	($func: ident($($arg: ident: $type: ty),*)) => {

		#[sync_task]
        pub fn $func(&mut self, $($arg: $type),*) -> Result<usize> {
			fn cancel(self: &mut Self) -> Result<()> {
				unsafe { self.io_engine.cancel(request.cast()) }
			}

			match unsafe { self.io_engine.$func($($arg),*, request) } {
				Some(result) => Progress::Done(result),
				None => Progress::Pending(cancel(self, request))
			}
        }
    }
}

pub struct Driver {
	io_engine: Box<dyn EngineImpl>,
	timers: BTreeSet<Timeout>,
	timer_count: u32 /* excludes idle timers */
}

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimeoutFlag {
	Abs  = 1 << 0,
	Idle = 1 << 1
}

struct Timeout {
	expire: u64,
	request: RequestPtr<Result<()>>,
	idle: bool
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

impl Driver {
	engine_task!(open(path: &CStr, flags: u32, mode: u32));

	engine_task!(close(fd: OwnedFd));

	engine_task!(read(fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64));

	engine_task!(write(fd: BorrowedFd<'_>, buf: &[u8], offset: i64));

	engine_task!(socket(domain: u32, socket_type: u32, protocol: u32));

	engine_task!(accept(socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32));

	engine_task!(connect(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32));

	engine_task!(recv(socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32));

	engine_task!(recvmsg(socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32));

	engine_task!(send(socket: BorrowedFd<'_>, buf: &[u8], flags: u32));

	engine_task!(sendmsg(socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32));

	engine_task!(shutdown(socket: BorrowedFd<'_>, how: Shutdown));

	engine_task!(bind(socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32));

	engine_task!(listen(socket: BorrowedFd<'_>, backlog: i32));

	pub fn new() -> Result<Self> {
		Ok(Self {
			timers: BTreeSet::new(),
			io_engine: Engine::new()?,
			timer_count: 0
		})
	}

	#[inline(always)]
	pub fn now() -> u64 {
		time::time(ClockId::Monotonic).expect("Failed to read the clock")
	}

	fn timer_complete(&mut self, timeout: Timeout, result: Result<()>) {
		if !timeout.idle {
			self.timer_count -= 1;
		}

		Request::complete(timeout.request, result);
	}

	/* inline never to prevent compiler from assuming state,
	 * see trait xx_core::task::env::Global
	 */
	#[inline(never)]
	fn expire_first_timer(&mut self) {
		let timeout = self.timers.pop_first().unwrap();

		self.timer_complete(timeout, Ok(()));
	}

	#[inline(never)]
	fn run_timers(&mut self) -> u64 {
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

				timeout = if timer.expire > now {
					timer.expire - now
				} else {
					0
				};

				break;
			}

			ran = true;

			self.expire_first_timer();
		}

		timeout
	}

	#[inline(never)]
	fn expire_all_timers(&mut self) {
		while self.timers.first().is_some() {
			self.expire_first_timer();
		}
	}

	fn queue_timer(&mut self, timer: Timeout) {
		if !timer.idle {
			self.timer_count += 1;
		}

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
	pub fn timeout(&mut self, mut expire: u64, flags: BitFlags<TimeoutFlag>) -> Result<()> {
		fn cancel(self: &mut Self, expire: u64) -> Result<()> {
			self.cancel_timer(Timeout { expire, request, idle: false })
		}

		if !flags.intersects(TimeoutFlag::Abs) {
			expire += Driver::now();
		}

		self.queue_timer(Timeout {
			expire,
			request,
			idle: flags.intersects(TimeoutFlag::Idle)
		});

		Progress::Pending(cancel(self, expire, request))
	}

	pub fn run(&mut self) -> Result<()> {
		loop {
			let timeout = self.run_timers();

			if self.timer_count == 0 && !self.io_engine.has_work() {
				break;
			}

			self.io_engine.work(timeout)?;
		}

		self.expire_all_timers();

		Ok(())
	}
}

impl Global for Driver {}