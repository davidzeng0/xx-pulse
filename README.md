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
async fn no_inline() {
	// the call to `no_inline()` is inlined,
	// but the await call to the body
	// will not be inlined
	...
}

#[inline(always)] // zero overhead calling!
#[asynchronous]
async fn always_inline() {
	// the code below will be inlined
	// into the calling function
	...
}
```

Async trait dynamic dispatch with zero allocations
```rust
#[asynchronous]
trait MyTrait {
	// dynamic dispatch! no `Box::new`
	async fn my_async_func(&mut self);

	// normal functions okay too
	fn my_normal_func(&mut self);
}

#[asynchronous]
async fn takes_trait(value: &mut dyn MyTrait) {
	value.my_async_func().await;
	value.my_normal_func();
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

Recursion (no allocation!)
```rust
#[asynchronous]
async fn fibonacci(n: i32) -> i32 {
	if n <= 2 {
		return 1;
	}

	fibonacci(n - 1).await + fibonacci(n - 2).await
}

println!("{}", fibonacci(20).await);
```