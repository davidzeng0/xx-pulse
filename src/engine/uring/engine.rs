use std::{
	cmp::max,
	ffi::CStr,
	io::{Error, ErrorKind, Result},
	mem::{size_of, zeroed},
	os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd},
	slice,
	sync::atomic::{AtomicU32, Ordering}
};

use enumflags2::{make_bitflags, BitFlags};
use xx_core::{
	os::{
		error::{result_from_int, ErrorCodes},
		io_uring::*,
		mman::*,
		openat::OpenAt,
		socket::{bind_raw, listen, MessageHeader, Shutdown}
	},
	pointer::{ConstPtr, MutPtr},
	task::{Request, RequestPtr},
	trace
};

use super::op::*;
use crate::engine::EngineImpl;

struct DualMapping {
	submission_ring: MemoryMap,
	completion_ring: MemoryMap
}

struct SingleMapping {
	ring: MemoryMap
}

enum RingMappings {
	Dual(DualMapping),
	Single(SingleMapping)
}

impl RingMappings {
	pub fn rings(&self) -> (&MemoryMap, &MemoryMap) {
		match self {
			RingMappings::Dual(map) => (&map.submission_ring, &map.completion_ring),
			RingMappings::Single(map) => (&map.ring, &map.ring)
		}
	}
}

struct Rings {
	rings: RingMappings,
	submission_entries: MemoryMap
}

impl Rings {
	fn map_memory(size: usize, offset: MmapOffsets, fd: BorrowedFd<'_>) -> Result<MemoryMap> {
		map_memory(
			0,
			size,
			make_bitflags!(MemoryProtection::{Read | Write}).bits(),
			MemoryType::Shared as u32 | MemoryFlag::Populate as u32,
			Some(fd),
			offset as isize
		)
	}

	fn scale<T>(mut count: u32, offset: u32, wide: bool) -> usize {
		if wide {
			count *= 2;
		}

		offset as usize + size_of::<T>() * count as usize
	}

	pub fn new(fd: BorrowedFd<'_>, params: &Parameters) -> Result<Self> {
		let ring_sizes = (
			Self::scale::<u32>(params.sq_entries, params.sq_off.array, false),
			Self::scale::<CompletionEntry>(
				params.cq_entries,
				params.cq_off.cqes,
				params.flags().intersects(SetupFlag::CompletionEntryWide)
			)
		);

		let rings = if params.features().intersects(Feature::SingleMmap) {
			RingMappings::Single(SingleMapping {
				ring: Self::map_memory(
					max(ring_sizes.0, ring_sizes.1),
					MmapOffsets::SubmissionRing,
					fd
				)?
			})
		} else {
			RingMappings::Dual(DualMapping {
				submission_ring: Self::map_memory(ring_sizes.0, MmapOffsets::SubmissionRing, fd)?,
				completion_ring: Self::map_memory(ring_sizes.1, MmapOffsets::CompletionRing, fd)?
			})
		};

		let submission_entries_size = Self::scale::<SubmissionEntry>(
			params.sq_entries,
			0,
			params.flags().intersects(SetupFlag::SubmissionEntryWide)
		);

		Ok(Self {
			rings,
			submission_entries: Self::map_memory(
				submission_entries_size,
				MmapOffsets::SubmissionEntries,
				fd
			)?
		})
	}
}

#[allow(dead_code)]

struct SubmissionQueue<'a> {
	pub khead: &'a mut AtomicU32,
	pub ktail: &'a mut AtomicU32,
	pub kflags: &'a mut AtomicU32,
	pub kdropped: &'a mut AtomicU32,

	pub array: &'a mut [u32],
	pub entries: &'a mut [SubmissionEntry],

	pub mask: u32,
	pub capacity: u32,

	pub tail: u32
}

#[allow(dead_code)]

struct CompletionQueue<'a> {
	pub khead: &'a mut AtomicU32,
	pub ktail: &'a mut AtomicU32,
	pub kflags: &'a mut AtomicU32,
	pub koverflow: &'a mut AtomicU32,

	pub entries: &'a mut [CompletionEntry],

	pub mask: u32,
	pub capacity: u32
}

#[allow(dead_code)]

struct Queue<'a> {
	pub rings: Rings,
	pub submission: SubmissionQueue<'a>,
	pub completion: CompletionQueue<'a>
}

fn get_ptr<T>(map: &MemoryMap, off: u32) -> MutPtr<T> {
	MutPtr::from(map.addr + off as usize)
}

fn get_ref<'a, T>(map: &MemoryMap, off: u32) -> &'a mut T {
	get_ptr::<T>(map, off).into_mut()
}

fn get_array<'a, T>(map: &MemoryMap, off: u32, len: u32) -> &'a mut [T] {
	unsafe { slice::from_raw_parts_mut(get_ptr::<T>(map, off).as_ptr_mut(), len as usize) }
}

#[allow(dead_code)]

impl<'a> SubmissionQueue<'a> {
	pub fn new(maps: &Rings, params: &Parameters) -> SubmissionQueue<'a> {
		let ring = maps.rings.rings().0;
		let array = get_array(ring, params.sq_off.array, params.sq_entries);

		for (i, elem) in array.iter_mut().enumerate() {
			*elem = i as u32;
		}

		SubmissionQueue {
			khead: get_ref(ring, params.sq_off.head),
			ktail: get_ref(ring, params.sq_off.tail),
			kflags: get_ref(ring, params.sq_off.flags),
			kdropped: get_ref(ring, params.sq_off.dropped),

			array,
			entries: get_array(&maps.submission_entries, 0, params.sq_entries),

			mask: params.sq_entries - 1,
			capacity: params.sq_entries,

			tail: 0
		}
	}

	pub fn flags(&self) -> BitFlags<SubmissionRingFlag> {
		let flags = self.kflags.load(Ordering::Relaxed);

		unsafe { BitFlags::from_bits_unchecked(flags) }
	}

	pub fn get_entry(&mut self, index: u32) -> &mut SubmissionEntry {
		unsafe { self.entries.get_unchecked_mut((index & self.mask) as usize) }
	}

	pub fn next(&mut self) -> &mut SubmissionEntry {
		let tail = self.tail;

		self.tail = self.tail.wrapping_add(1);
		self.get_entry(tail & self.mask)
	}

	pub fn sync(&mut self) {
		self.ktail.store(self.tail, Ordering::Relaxed);
	}
}

#[allow(dead_code)]

impl<'a> CompletionQueue<'a> {
	pub fn new(maps: &Rings, params: &Parameters) -> CompletionQueue<'a> {
		let ring = maps.rings.rings().1;

		CompletionQueue {
			khead: get_ref(ring, params.cq_off.head),
			ktail: get_ref(ring, params.cq_off.tail),
			kflags: get_ref(ring, params.cq_off.flags),
			koverflow: get_ref(ring, params.cq_off.overflow),

			entries: get_array(ring, params.cq_off.cqes, params.cq_entries),

			mask: params.cq_entries - 1,
			capacity: params.cq_entries
		}
	}

	pub fn flags(&self) -> BitFlags<SubmissionRingFlag> {
		let flags = self.kflags.load(Ordering::Relaxed);

		unsafe { BitFlags::from_bits_unchecked(flags) }
	}

	pub fn get_entry(&mut self, index: u32) -> &mut CompletionEntry {
		unsafe { self.entries.get_unchecked_mut(index as usize) }
	}

	pub fn read_ring(&self) -> (u32, u32) {
		(
			unsafe { *self.khead.as_ptr() },
			self.ktail.load(Ordering::Acquire)
		)
	}
}

impl<'a> Queue<'a> {
	pub fn new(rings: Rings, params: Parameters) -> Queue<'a> {
		Queue {
			submission: SubmissionQueue::new(&rings, &params),
			completion: CompletionQueue::new(&rings, &params),
			rings
		}
	}

	pub fn needs_flush(&self) -> bool {
		self.submission
			.flags()
			.intersects(make_bitflags!(SubmissionRingFlag::{CqOverflow}))
	}

	pub fn needs_enter(&self) -> bool {
		self.submission
			.flags()
			.intersects(make_bitflags!(SubmissionRingFlag::{CqOverflow | TaskRun}))
	}
}

pub struct IoUring<'a> {
	ring_fd: OwnedFd,
	queue: Queue<'a>,

	to_complete: u64,
	to_submit: u32,

	no_op_req: Request<Result<usize>>
}

fn no_op(_: RequestPtr<Result<usize>>, _: *const (), _: Result<usize>) {}

impl<'a> IoUring<'a> {
	pub fn new() -> Result<Self> {
		let mut params: Parameters = unsafe { zeroed() };

		params.sq_entries = 256;
		params.cq_entries = 65536;
		params.set_flags(make_bitflags!(SetupFlag::{
			CompletionRingSize | Clamp | SubmitAll | CoopTaskrun | TaskRun | SingleIssuer | DeferTaskrun
		}));

		let ring_fd = io_uring_setup(params.sq_entries, &mut params)?;
		let rings = Rings::new(ring_fd.as_fd(), &params)?;
		let queue = Queue::new(rings, params);

		Ok(Self {
			ring_fd,
			queue,
			to_submit: 0,
			to_complete: 0,
			no_op_req: unsafe { Request::new(ConstPtr::<()>::null().as_raw_ptr(), no_op) }
		})
	}

	#[inline(always)]
	fn enter<F: FnOnce(&mut Self) -> Result<i32>>(&mut self, f: F) -> Result<()> {
		self.queue.submission.sync();

		let submitted = match f(self) {
			Ok(count) => count,
			Err(err) => {
				const TIMED_OUT: i32 = ErrorCodes::Time as i32;
				const CQ_OVERFLOW: i32 = ErrorCodes::Busy as i32;
				const INTERRUPTED: i32 = ErrorCodes::Intr as i32;
				const NO_MEM: i32 = ErrorCodes::Again as i32;

				let code = err.raw_os_error().unwrap();

				match code {
					NO_MEM => -1,
					TIMED_OUT | CQ_OVERFLOW | INTERRUPTED => {
						if self.to_submit == 0 {
							return Ok(());
						}

						-1
					}

					_ => {
						return Err(err);
					}
				}
			}
		};

		if self.to_submit != 0 {
			if submitted != self.to_submit as i32 {
				return Err(Error::new(
					ErrorKind::OutOfMemory,
					format!("Submitted {} / {}", submitted, self.to_submit)
				));
			}

			trace!(target: self, "<< {} Operations", submitted);

			self.to_submit = 0;
			self.to_complete += submitted as u64;
		}

		Ok(())
	}

	#[inline(always)]
	fn flush(&mut self) -> Result<()> {
		let mut flags = BitFlags::<EnterFlag>::default();

		/* we want to flush cqring if possible, but not run any task work */
		if self.queue.needs_flush() {
			flags |= EnterFlag::GetEvents;
		}

		self.enter(|this| {
			io_uring_enter2(
				this.ring_fd.as_fd(),
				this.to_submit,
				0,
				flags.bits(),
				0,
				SIGSET_SIZE
			)
		})
	}

	#[inline(always)]
	fn submit_and_wait(&mut self, timeout: u64) -> Result<(u32, u32)> {
		let flags = make_bitflags!(EnterFlag::{GetEvents});
		let mut wait = 0;

		if timeout != 0 {
			wait = 1;
		} else if self.to_submit == 0 {
			let ring = self.queue.completion.read_ring();

			if ring.0 != ring.1 {
				/* already have completions */
				return Ok(ring);
			}

			if !self.queue.needs_enter() {
				/* no pending completions, no submissions, nothing to wait for, nothing to */
				return Ok(ring);
			}
		}

		self.enter(|this| {
			/*
			 * the kernel doesn't read the timespec until it's actually time to wait for
			 * cqes avoid loss due to branching here and set EXT_ARG on every enter
			 */
			io_uring_enter_timeout(
				this.ring_fd.as_fd(),
				this.to_submit,
				wait,
				flags.bits(),
				timeout
			)
		})?;

		Ok(self.queue.completion.read_ring())
	}

	#[inline(always)]
	fn run_events(&mut self, (mut head, tail): (u32, u32)) {
		if tail == head {
			return;
		}

		let mask = self.queue.completion.mask;

		trace!(target: self, ">> {} Completions", tail.wrapping_sub(head));

		while tail != head {
			let CompletionEntry { user_data, result, .. } =
				*self.queue.completion.get_entry(head & mask);

			/*
			 * more requests may be queued in callback, so
			 * update the cqe head here so that we have one more cqe
			 * available for completions before overflow occurs
			 */
			head = head.wrapping_add(1);
			self.queue.completion.khead.store(head, Ordering::Release);
			self.to_complete -= 1;

			Request::complete(
				ConstPtr::from(user_data as *const Request<Result<usize>>),
				result_from_int(result as isize).map(|result| result as usize)
			);
		}
	}

	#[inline(always)]
	pub fn push(&mut self, request: &SubmissionEntry) {
		*self.queue.submission.next() = *request;
		self.to_submit += 1;

		if self.to_submit >= self.queue.submission.capacity {
			self.flush().expect("Failed to flush submission ring");
		}
	}
}

impl EngineImpl for IoUring<'_> {
	fn has_work(&self) -> bool {
		self.to_complete != 0 || self.to_submit != 0
	}

	#[inline(always)]
	fn work(&mut self, timeout: u64) -> Result<()> {
		let events = self.submit_and_wait(timeout).expect("Failed to get events");
		self.run_events(events);

		Ok(())
	}

	#[inline(always)]
	unsafe fn cancel(&mut self, request: RequestPtr<()>) -> Result<()> {
		let mut op = Op::cancel(0);

		op.addr.addr = request.as_raw_int() as u64;
		op.user_data = ConstPtr::from(&self.no_op_req).as_raw_int() as u64;

		self.push(&op);

		Ok(())
	}

	#[inline(always)]
	unsafe fn open(
		&mut self, path: &CStr, flags: u32, mode: u32, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::openat(OpenAt::CurrentWorkingDirectory as i32, path, flags, mode, 0);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn close(
		&mut self, fd: OwnedFd, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		/* into is safe here because push panics if out of memory */
		let mut op = Op::close(fd.into_raw_fd());

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn read(
		&mut self, fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::read(
			fd.as_raw_fd(),
			buf.as_ptr() as usize,
			buf.len() as u32,
			offset,
			0
		);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn write(
		&mut self, fd: BorrowedFd<'_>, buf: &[u8], offset: i64, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::write(
			fd.as_raw_fd(),
			buf.as_ptr() as usize,
			buf.len() as u32,
			offset,
			0
		);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn socket(
		&mut self, domain: u32, socket_type: u32, protocol: u32, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::socket(domain, socket_type, protocol, 0, 0);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn accept(
		&mut self, socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::accept(socket.as_raw_fd(), addr.as_raw_int(), addrlen, 0, 0);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn connect(
		&mut self, socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::connect(socket.as_raw_fd(), addr.as_raw_int(), addrlen);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn recv(
		&mut self, socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::recv(
			socket.as_raw_fd(),
			buf.as_mut_ptr() as usize,
			buf.len() as u32,
			flags
		);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn recvmsg(
		&mut self, socket: BorrowedFd<'_>, header: &mut MessageHeader, flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::recvmsg(socket.as_raw_fd(), header, flags);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn send(
		&mut self, socket: BorrowedFd<'_>, buf: &[u8], flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::send(
			socket.as_raw_fd(),
			buf.as_ptr() as usize,
			buf.len() as u32,
			flags
		);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn sendmsg(
		&mut self, socket: BorrowedFd<'_>, header: &MessageHeader, flags: u32,
		request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::sendmsg(socket.as_raw_fd(), header, flags);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn shutdown(
		&mut self, socket: BorrowedFd<'_>, how: Shutdown, request: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		let mut op = Op::shutdown(socket.as_raw_fd(), how);

		op.user_data = request.as_raw_int() as u64;

		self.push(&op);

		None
	}

	#[inline(always)]
	unsafe fn bind(
		&mut self, socket: BorrowedFd<'_>, addr: ConstPtr<()>, addrlen: u32,
		_: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		Some(bind_raw(socket, addr, addrlen).map(|_| 0))
	}

	#[inline(always)]
	unsafe fn listen(
		&mut self, socket: BorrowedFd<'_>, backlog: i32, _: RequestPtr<Result<usize>>
	) -> Option<Result<usize>> {
		Some(listen(socket, backlog).map(|_| 0))
	}
}
