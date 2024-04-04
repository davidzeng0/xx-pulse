# xx-pulse

![](https://github.com/davidzeng0/xx-pulse/actions/workflows/build.yml/badge.svg?event=push)

The fastest async runtime for rust

Available I/O Backends:
- io_uring (requires linux kernel version >= 5.6, recommended 5.11 or 6.1 for best performance)
- kqueue, iocp, epoll: contributions welcome

### Note:
This library is currently only available for Linux (contributions welcome).<br>
For Windows and Mac users, running in Docker or WSL also work.

### Why Fibers?

Performance (inlining)
```rust
#[inline(never)] // applies the inline to the `.await`!
#[asynchronous]
async fn no_inline() { ... }

#[inline(always)] // zero overhead calling!
#[asynchronous]
async fn always_inline() { ... }
```

Async trait dynamic dispatch with zero allocations
```rust
#[asynchronous]
trait MyTrait {
	fn my_func(&mut self); // dynamic dispatch! no `Box::new`
}
```

Mix sync code and async code
```rust
struct AsyncReader {
	async_context: Ptr<Context>
}

impl std::io::Read for AsyncReader {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		#[asynchronous]
		async fn read(buf: &mut [u8]) { ... }

		/* Safety: must ensure any lifetimes are valid across suspends */
		unsafe {
			with_context(
				self.async_context,
				read(buf)
			)
		}
	}
}
```

Async closures (no more poll functions!)
```rust
#[asynchronous]
async fn call_async_closure(mut func: impl AsyncFnMut<i32>) {
	func.call_mut(5).await;
	func.call_mut(42).await;
}

let mut num = 0;

// note: rustc complains that its unstable, even though
// the proc macro modifies the syntax
//
// use `|| async move`, which does the same thing, to remove
// the error
call_async_closure(async |val| {
	num += val;
}).await;

println!("{num}!"); // 47!
```

Recursion (no allocation)
```rust
#[asynchronous]
async fn fibonacci(n: i32) -> i32 {
	if n <= 2 {
		return 1;
	}

	fibonacci(n - 1).await + fibonacci(n - 2).await
}
```