# xx-pulse

![](https://github.com/davidzeng0/xx-pulse/actions/workflows/build.yml/badge.svg?event=push)

`msrv: 1.76.0 stable`

The fastest async runtime for rust (see [benchmarks](./benchmarks/README.md))

Available I/O Backends:
- io_uring (requires linux kernel version >= 5.6, recommended 5.11 or 6.1 for best performance)
- kqueue, iocp, epoll: contributions welcome

Currently supported architectures:
- amd64 (x86_64)
- arm64 (aarch64)

Memory safety status: FFI support required in Miri, currently checked by manual code review and tests

Current and planned features:
- [x] custom async desugaring
- [x] low latency and zero overhead read/write APIs with the same ergonomics as `std::io`, now with 100% less allocations, `thread_local` lookups, memory copy, and passing owned buffers
- [x] dynamic dispatch
- [x] low overhead spawn, select, join
- [x] safe async closures, async fn mut
- [x] use sync code in an async manner, without rewriting existing sync code
- [ ] async drop (compiler support is ideal)
- [ ] io_uring goodies (fixed file, buffer select, async drop support is ideal to implement ergonomically)

### Note:
This library is currently only available for Linux (contributions welcome).<br>
For Windows and Mac users, running in Docker or WSL also work.

### Echo server example

Add dependency
```sh
# support lib and async impls
cargo add --git https://github.com/davidzeng0/xx-core.git xx-core

# i/o engine and driver
cargo add --git https://github.com/davidzeng0/xx-pulse.git xx-pulse
```

In file `main.rs`
```rust
use xx_pulse::Tcp;

#[xx_pulse::main]
async fn main() {
    let listener = Tcp::bind("127.0.0.1:8080").await.unwrap();

    loop {
        let (mut client, _) = listener.accept().await.unwrap();

        xx_pulse::spawn(async move {
            let mut buf = [0; 1024];

            loop {
                let n = client.recv(&mut buf, Default::default()).await.unwrap();

                if n == 0 {
                    break;
                }

                let n = client.send(&buf, Default::default()).await.unwrap();

                if n == 0 {
                    break;
                }
            }
        }).await;
    }
}
```

### Why Fibers?

Performance (inlining) (also see [switching](https://github.com/davidzeng0/xx-core/blob/main/src/coroutines/README.md))
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
            scoped(
                self.async_context,
                read(buf)
            )
        }
    }
}
```

Async closures (no more poll functions or `impl Future for MyFuture`!)
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

println!("{}", fibonacci(20).await); // 6765
```