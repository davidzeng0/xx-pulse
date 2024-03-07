use std::{
	ffi::CStr,
	mem::size_of,
	os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd},
	slice,
	sync::atomic::{AtomicU32, Ordering}
};

use enumflags2::{make_bitflags, BitFlags};
use xx_core::{
	error,
	error::*,
	future::*,
	opt::hint::*,
	os::{error::*, io_uring::*, mman::*, openat::*, socket::*, stat::*, time::*},
	pointer::*,
	trace, warn
};

use super::*;

struct Rings<'a> {
	ring: MemoryMap<'a>,
	separate_completion_ring: Option<MemoryMap<'a>>,
	submission_entries: MemoryMap<'a>
}

impl<'a> Rings<'a> {
	fn scale<T>(mut count: u32, offset: u32, wide: bool) -> usize {
		if wide {
			count *= 2;
		}

		offset as usize + size_of::<T>() * count as usize
	}

	fn map_memory(size: usize, offset: MmapOffsets, fd: BorrowedFd<'_>) -> Result<MemoryMap<'a>> {
		MemoryMap::map(
			None,
			size,
			make_bitflags!(MemoryProtection::{Read | Write}).bits(),
			MemoryType::Shared as u32 | MemoryFlag::Populate as u32,
			Some(fd),
			offset as isize
		)
	}

	fn submission_ring(&self) -> &MemoryMap<'a> {
		&self.ring
	}

	fn completion_ring(&self) -> &MemoryMap<'a> {
		self.separate_completion_ring.as_ref().unwrap_or(&self.ring)
	}

	fn new(fd: BorrowedFd<'_>, params: &Parameters) -> Result<Self> {
		let ring_sizes = (
			Self::scale::<u32>(params.sq_entries, params.sq_off.array, false),
			Self::scale::<CompletionEntry>(
				params.cq_entries,
				params.cq_off.cqes,
				params.flags().intersects(SetupFlag::CompletionEntryWide)
			)
		);

		let (ring, separate_completion_ring) = if params.features().intersects(Feature::SingleMmap)
		{
			(
				Self::map_memory(
					ring_sizes.0.max(ring_sizes.1),
					MmapOffsets::SubmissionRing,
					fd
				)?,
				None
			)
		} else {
			(
				Self::map_memory(ring_sizes.0, MmapOffsets::SubmissionRing, fd)?,
				Some(Self::map_memory(
					ring_sizes.1,
					MmapOffsets::CompletionRing,
					fd
				)?)
			)
		};

		let submission_entries_size = Self::scale::<SubmissionEntry>(
			params.sq_entries,
			0,
			params.flags().intersects(SetupFlag::SubmissionEntryWide)
		);

		Ok(Self {
			ring,
			separate_completion_ring,
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
	khead: &'a mut AtomicU32,
	ktail: &'a mut AtomicU32,
	kflags: &'a mut AtomicU32,
	kdropped: &'a mut AtomicU32,

	array: &'a mut [u32],
	entries: &'a mut [SubmissionEntry],

	mask: u32,
	capacity: u32,

	tail: u32
}

#[allow(dead_code)]
struct CompletionQueue<'a> {
	khead: &'a mut AtomicU32,
	ktail: &'a mut AtomicU32,
	kflags: &'a mut AtomicU32,
	koverflow: &'a mut AtomicU32,

	entries: &'a mut [CompletionEntry],

	mask: u32,
	capacity: u32
}

#[allow(dead_code)]
struct Queue<'a> {
	rings: Rings<'a>,
	submission: SubmissionQueue<'a>,
	completion: CompletionQueue<'a>
}

fn get_ptr<'a, T>(map: &MemoryMap<'a>, off: u32) -> MutPtr<T> {
	map.addr().cast::<u8>().add(off as usize).cast()
}

unsafe fn get_ref<'a, T>(map: &MemoryMap<'a>, off: u32) -> &'a mut T {
	get_ptr::<T>(map, off).as_mut()
}

unsafe fn get_array<'a, T>(map: &MemoryMap<'a>, off: u32, len: u32) -> &'a mut [T] {
	slice::from_raw_parts_mut(get_ref::<T>(map, off), len as usize)
}

impl<'a> SubmissionQueue<'a> {
	unsafe fn new(maps: &Rings<'a>, params: &Parameters) -> SubmissionQueue<'a> {
		let ring = maps.submission_ring();
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

	fn flags(&self) -> BitFlags<SubmissionRingFlag> {
		let flags = self.kflags.load(Ordering::Relaxed);

		unsafe { BitFlags::from_bits_unchecked(flags) }
	}

	unsafe fn get_entry(&mut self, index: u32) -> &mut SubmissionEntry {
		self.entries.get_unchecked_mut((index & self.mask) as usize)
	}

	fn next(&mut self) -> &mut SubmissionEntry {
		let tail = self.tail;

		self.tail = self.tail.wrapping_add(1);

		unsafe { self.get_entry(tail & self.mask) }
	}

	fn sync(&mut self) {
		self.ktail.store(self.tail, Ordering::Relaxed);
	}
}

#[allow(dead_code)]
impl<'a> CompletionQueue<'a> {
	unsafe fn new(maps: &Rings<'a>, params: &Parameters) -> CompletionQueue<'a> {
		let ring = maps.completion_ring();

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

	fn flags(&self) -> BitFlags<CompletionRingFlag> {
		let flags = self.kflags.load(Ordering::Relaxed);

		unsafe { BitFlags::from_bits_unchecked(flags) }
	}

	unsafe fn get_entry(&mut self, index: u32) -> &mut CompletionEntry {
		self.entries.get_unchecked_mut(index as usize)
	}

	fn read_ring(&self) -> (u32, u32) {
		(
			unsafe { *self.khead.as_ptr() },
			self.ktail.load(Ordering::Acquire)
		)
	}
}

impl<'a> Queue<'a> {
	unsafe fn new(rings: Rings<'a>, params: Parameters) -> Queue<'a> {
		Queue {
			submission: SubmissionQueue::new(&rings, &params),
			completion: CompletionQueue::new(&rings, &params),
			rings
		}
	}

	fn needs_flush(&self) -> bool {
		self.submission
			.flags()
			.intersects(make_bitflags!(SubmissionRingFlag::{CqOverflow}))
	}

	fn needs_enter(&self) -> bool {
		self.submission
			.flags()
			.intersects(make_bitflags!(SubmissionRingFlag::{CqOverflow | TaskRun}))
	}
}

pub struct IoUring {
	features: IoRingFeatures,
	ring_fd: OwnedFd,
	queue: Queue<'static>,

	to_complete: u64,
	to_submit: u32
}

const NO_OP: Request<isize> = Request::no_op();

fn create_io_uring() -> Result<(IoRingFeatures, OwnedFd, Parameters)> {
	struct IoUringSetup {}

	let ring = IoUringSetup {};

	let features = match io_uring_detect_features()? {
		Some(features) => features,
		None => {
			error!(target: &ring,
				"== Failed to setup io_uring.\n\
				:: The current linux kernel does not support io_uring.\n\
				:: Please upgrade the kernel to a minimum of version 5.11 (recommended >= 6.1) and try again."
			);

			return Err(OsError::NoSys.into());
		}
	};

	let ops = [
		(OpCode::OpenAt, Some("files"), None),
		(OpCode::Close, None, Some("Some operations may be blocking")),
		(OpCode::Read, Some("files"), None),
		(OpCode::Write, Some("files"), None),
		(OpCode::Socket, None, Some("Using syscall as fallback")),
		(OpCode::Accept, Some("sockets"), None),
		(OpCode::Connect, Some("sockets"), None),
		(OpCode::Recv, Some("sockets"), None),
		(OpCode::RecvMsg, Some("sockets"), None),
		(OpCode::Send, Some("sockets"), None),
		(OpCode::SendMsg, Some("sockets"), None),
		(
			OpCode::Shutdown,
			None,
			Some("Some operations may be blocking")
		),
		(OpCode::FileSync, Some("files"), None),
		(OpCode::Statx, Some("files"), None),
		(OpCode::PollAdd, None, None)
	];

	for (op, feature, message) in &ops {
		if !features.opcode_supported(*op) {
			let feature = feature
				.map(|feat| format!("(like {})", feat))
				.unwrap_or("".to_string());

			warn!(
				target: &ring,
				"== Op code `{:?}`, is not supported.\n\
				:: {}.\n\
				:: Linux kernel version 6.1 or greater is recommended.",
				*op,
				message.unwrap_or(&format!(
					"Some features {} may not be available",
					feature
				))
			);
		}
	}

	let mut setup_flags = BitFlags::default();
	let mut params = Parameters::default();

	let flags = make_bitflags!(SetupFlag::{CompletionRingSize | Clamp | SubmitAll | CoopTaskrun | TaskRun | SingleIssuer | DeferTaskrun});

	for flag in flags {
		if features.setup_flag_supported(flag) {
			setup_flags |= flag;
		}
	}

	params.sq_entries = 256;
	params.cq_entries = 65536;
	params.set_flags(setup_flags);

	if !setup_flags.intersects(SetupFlag::Clamp) {
		params.cq_entries = 0;
	}

	if !setup_flags.contains(flags) || !features.feature_supported(Feature::ExtArg) {
		warn!(
			target: &ring,
			"== Running in compatibility mode on an estimated linux kernel version of {}.\n\
			:: The preferred version is atleast 6.1. Some features may not be available.\n\
			:: Performance may be degraded.",
			features.version()
		);
	}

	match io_uring_setup(params.sq_entries, &mut params) {
		Ok(fd) => {
			trace!(
				target: &ring,
				"++ Initialized with {}:{} entries",
				params.sq_entries,
				params.cq_entries
			);

			Ok((features, fd, params))
		}

		Err(err) => {
			match err.os_error().unwrap() {
				OsError::NoMem => error!(target: &ring,
					"== Failed to setup io_uring.\n\
					:: This is usually because the current locked memory limit is too low.\n\
					:: Please raise the limit and try again."
				),
				_ => ()
			}

			Err(err)
		}
	}
}

impl IoUring {
	pub fn new() -> Result<Self> {
		let (features, ring_fd, params) = create_io_uring()?;
		let rings = Rings::new(ring_fd.as_fd(), &params)?;
		let queue = unsafe { Queue::new(rings, params) };

		Ok(Self {
			features,
			ring_fd,
			queue,
			to_submit: 0,
			to_complete: 0
		})
	}

	#[inline(never)]
	fn enter_cold(&mut self, err: Option<Error>) -> Result<()> {
		let err = match err {
			None => return Err(Core::OutOfMemory.as_err()),
			Some(err) => err.os_error().unwrap()
		};

		match err {
			OsError::Time | OsError::Intr | OsError::Busy if self.to_submit == 0 => return Ok(()),
			OsError::Again => return Err(Core::OutOfMemory.as_err()),
			_ => ()
		}

		Err(err.into())
	}

	#[inline(always)]
	fn enter<F: Fn(&mut Self) -> Result<i32>>(&mut self, f: F) -> Result<()> {
		self.queue.submission.sync();

		if self.to_submit != 0 {
			trace!(target: self, "<< {} Operations", self.to_submit);
		}

		loop {
			let err = match f(self) {
				Ok(submitted) => {
					self.to_submit -= submitted as u32;
					self.to_complete += submitted as u64;

					if likely(self.to_submit == 0) {
						break;
					}

					if likely(!self.features.setup_flag_supported(SetupFlag::SubmitAll)) {
						continue;
					}

					None
				}

				Err(err) => Some(err)
			};

			self.enter_cold(err)?;

			break;
		}

		Ok(())
	}

	#[inline(never)]
	fn flush(&mut self) -> Result<()> {
		let mut flags = BitFlags::<EnterFlag>::default();

		/* we want to flush cqring if possible, but not run any task work */
		if self.queue.needs_flush() {
			flags |= EnterFlag::GetEvents;
		}

		self.enter(|this| unsafe {
			io_uring_enter(
				this.ring_fd.as_fd(),
				this.to_submit,
				0,
				flags.bits(),
				MutPtr::null()
			)
		})
	}

	/// Compatibility function
	#[inline(never)]
	fn enter_timeout(&mut self, timeout: u64) -> Result<()> {
		let tail = self.queue.completion.read_ring().1;

		self.flush()?;

		/* some requests may have completed from `flush()`, if there are no
		 * completions, our timeout might hang. if we received events, there is no
		 * need to timeout anyway. note that this is a potential race condition.
		 * the application may hang indefinitely when trying to exit if there are no
		 * cqes to be posted after the timeout gets queued
		 */
		if self.queue.completion.read_ring().1 != tail {
			return Ok(());
		}

		let ts = TimeSpec { nanos: timeout as i64, sec: 0 };
		let mut op = Op::timeout(&ts, 1, 0);

		self.push_with_request(&mut op, Ptr::from(&NO_OP));
		self.enter(|this| unsafe {
			io_uring_enter(
				this.ring_fd.as_fd(),
				this.to_submit,
				1,
				EnterFlag::GetEvents as u32,
				MutPtr::null()
			)
		})
		.unwrap();

		/* the kernel received our timeout, we can safely release `ts` */
		Ok(())
	}

	fn submit_and_wait(&mut self, timeout: u64) -> Result<(u32, u32)> {
		let mut wait = 0;

		if likely(timeout != 0) {
			wait = 1;
		} else if unlikely(self.to_submit == 0) {
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

		if likely(self.features.feature_supported(Feature::ExtArg)) {
			self.enter(|this| unsafe {
				/*
				 * the kernel doesn't read the timespec until it's actually time to wait for
				 * cqes. avoid loss due to branching here and set EXT_ARG on every enter
				 */
				io_uring_enter_timeout(
					this.ring_fd.as_fd(),
					this.to_submit,
					wait,
					EnterFlag::GetEvents as u32,
					timeout
				)
			})?;
		} else {
			self.enter_timeout(timeout)?;
		}

		Ok(self.queue.completion.read_ring())
	}

	#[inline(always)]
	fn run_events(&mut self, (mut head, tail): (u32, u32)) {
		let mask = self.queue.completion.mask;

		if tail != head {
			trace!(target: self, ">> {} Completions", tail.wrapping_sub(head));
		}

		self.to_complete -= tail.wrapping_sub(head) as u64;

		while tail != head {
			let CompletionEntry { user_data, result, .. } =
				*unsafe { self.queue.completion.get_entry(head & mask) };
			/*
			 * more requests may be queued in callback, so
			 * update the cqe head here so that we have one more cqe
			 * available for completions before overflow occurs
			 */
			head = head.wrapping_add(1);
			self.queue.completion.khead.store(head, Ordering::Release);

			unsafe { Request::complete(Ptr::from_int_addr(user_data as usize), result as isize) };
		}
	}

	#[inline(always)]
	fn push(&mut self, request: &SubmissionEntry) {
		*self.queue.submission.next() = *request;
		self.to_submit += 1;

		if unlikely(self.to_submit >= self.queue.submission.capacity) {
			self.flush().expect("Failed to flush submission ring");
		}
	}

	#[inline(always)]
	fn push_with_request(&mut self, op: &mut SubmissionEntry, request: ReqPtr<isize>) {
		op.user_data = request.int_addr() as u64;

		self.push(op);
	}
}

impl EngineImpl for IoUring {
	#[inline(always)]
	fn has_work(&self) -> bool {
		self.to_complete != 0 || self.to_submit != 0
	}

	fn work(&mut self, timeout: u64) -> Result<()> {
		let events = self.submit_and_wait(timeout).expect("Failed to get events");

		self.run_events(events);

		Ok(())
	}

	unsafe fn cancel(&mut self, request: ReqPtr<()>) -> Result<()> {
		let mut op = Op::cancel(0);

		op.addr.addr = request.int_addr() as u64;

		self.push_with_request(&mut op, Ptr::from(&NO_OP));

		Ok(())
	}

	unsafe fn open(
		&mut self, path: &CStr, flags: u32, mode: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::openat(OpenAt::CurrentWorkingDirectory as i32, path, flags, mode, 0);

		self.push_with_request(&mut op, request);

		None
	}

	fn close_kind(&self) -> OperationKind {
		if unlikely(!self.features.opcode_supported(OpCode::Close)) {
			OperationKind::SyncOffload
		} else {
			OperationKind::Async
		}
	}

	unsafe fn close(&mut self, fd: OwnedFd, request: ReqPtr<isize>) -> Option<isize> {
		if unlikely(!self.features.opcode_supported(OpCode::Close)) {
			return SyncEngine {}.close(fd, request);
		}

		/* into is safe here because push panics if out of memory, and we don't
		 * handle panics */
		let mut op = Op::close(fd.into_raw_fd());

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn read(
		&mut self, fd: BorrowedFd<'_>, buf: &mut [u8], offset: i64, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::read(
			fd.as_raw_fd(),
			MutPtr::from(buf.as_mut_ptr()).as_unit(),
			buf.len().min(u32::MAX as usize) as u32,
			offset,
			0
		);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn write(
		&mut self, fd: BorrowedFd<'_>, buf: &[u8], offset: i64, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::write(
			fd.as_raw_fd(),
			Ptr::from(buf.as_ptr()).as_unit(),
			buf.len().min(u32::MAX as usize) as u32,
			offset,
			0
		);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn socket(
		&mut self, domain: u32, socket_type: u32, protocol: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		if unlikely(!self.features.opcode_supported(OpCode::Socket)) {
			return SyncEngine {}.socket(domain, socket_type, protocol, request);
		}

		let mut op = Op::socket(domain, socket_type, protocol, 0, 0);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn accept(
		&mut self, socket: BorrowedFd<'_>, addr: MutPtr<()>, addrlen: &mut u32,
		request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::accept(socket.as_raw_fd(), addr.int_addr(), addrlen, 0, 0);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn connect(
		&mut self, socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::connect(socket.as_raw_fd(), addr.int_addr(), addrlen);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn recv(
		&mut self, socket: BorrowedFd<'_>, buf: &mut [u8], flags: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::recv(
			socket.as_raw_fd(),
			buf.as_mut_ptr() as usize,
			buf.len().min(u32::MAX as usize) as u32,
			flags
		);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn recvmsg(
		&mut self, socket: BorrowedFd<'_>, header: &mut MessageHeaderMut<'_>, flags: u32,
		request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::recvmsg(socket.as_raw_fd(), header, flags);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn send(
		&mut self, socket: BorrowedFd<'_>, buf: &[u8], flags: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::send(
			socket.as_raw_fd(),
			buf.as_ptr() as usize,
			buf.len().min(u32::MAX as usize) as u32,
			flags
		);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn sendmsg(
		&mut self, socket: BorrowedFd<'_>, header: &MessageHeader<'_>, flags: u32,
		request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::sendmsg(socket.as_raw_fd(), header, flags);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn shutdown(
		&mut self, socket: BorrowedFd<'_>, how: Shutdown, request: ReqPtr<isize>
	) -> Option<isize> {
		if unlikely(!self.features.opcode_supported(OpCode::Shutdown)) {
			return SyncEngine {}.shutdown(socket, how, request);
		}

		let mut op = Op::shutdown(socket.as_raw_fd(), how);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn bind(
		&mut self, socket: BorrowedFd<'_>, addr: Ptr<()>, addrlen: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		SyncEngine {}.bind(socket, addr, addrlen, request)
	}

	unsafe fn listen(
		&mut self, socket: BorrowedFd<'_>, backlog: i32, request: ReqPtr<isize>
	) -> Option<isize> {
		SyncEngine {}.listen(socket, backlog, request)
	}

	unsafe fn fsync(&mut self, file: BorrowedFd<'_>, request: ReqPtr<isize>) -> Option<isize> {
		let mut op = Op::fsync(file.as_raw_fd(), 0);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn statx(
		&mut self, dirfd: Option<BorrowedFd<'_>>, path: &CStr, flags: u32, mask: u32,
		statx: &mut Statx, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::statx(
			dirfd
				.map(|fd| fd.as_raw_fd())
				.unwrap_or(OpenAt::CurrentWorkingDirectory as i32),
			path,
			flags,
			mask,
			statx
		);

		self.push_with_request(&mut op, request);

		None
	}

	unsafe fn poll(
		&mut self, fd: BorrowedFd<'_>, mask: u32, request: ReqPtr<isize>
	) -> Option<isize> {
		let mut op = Op::poll(fd.as_raw_fd(), mask);

		self.push_with_request(&mut op, request);

		None
	}
}
