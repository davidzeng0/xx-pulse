# xx-pulse

![](https://github.com/davidzeng0/xx-pulse/actions/workflows/build.yml/badge.svg?event=push)

`msrv: 1.80.0 stable`
(nightly is necessary until 1.80.0 is released as stable, but no nightly features are used)

Safe, [performant](./benchmarks/README.md), and efficient async rust runtime. <br>
Same syntax as async rust and zero-cost ergonomics. <br>
Based on stackful coroutines. <br>

- [Benchmarks](./benchmarks/README.md) <br>
- [Motivation and use cases](./Motivation.md) <br>
- [Getting started](#getting-started)
- [Thread local safety](#thread-local-safety)
- [Features and development stage](#features-and-development-stage)

### Note:

This library is not ready for production use. Many semantics and APIs are still under development.

This library is currently only available for Linux (other OS's contributions are welcome).<br>
For Windows and Mac users, running in Docker or WSL also work. See [features and development stage](#features-and-development-stage)

The [rust](https://hub.docker.com/_/rust) docker container is sufficient (needs nightly installation).

### Getting started

The following is a simple echo server example

#### Add dependency
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

    // Can also timeout operations after 5s
    let listener: Option<Result<TcpListener, _>> = Tcp::bind("...")
        .timeout(xx_core::duration!(5 s))
        .await;

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

### Thread local safety

Thread local access is safe because of async/await syntax. <br>
A compiler error prevents usage of `.await` in synchronous functions and closures. <br>
xx-pulse uses cooperative scheduling, so it is impossble to suspend in a closure without using `unsafe`. <br>
To suspend anyway, see [using sync code as if it were async](./Motivation.md#use-sync-code-as-if-it-were-async)

```rust
#[asynchronous]
async fn try_use_thread_local() {
	THREAD_LOCAL.with(|value| {
		// Ok, cannot cause UB
		use_value_synchronously(value);
	});

	THREAD_LOCAL.with(|value| {
		// Compiler error: cannot use .await in a synchronous function/closure!
		do_async_stuff().await;
	});
}
```

### Features and development stage

Available I/O Backends:
- io_uring (requires linux kernel version >= 5.6, recommended 5.11 or 6.1 for best performance)
- kqueue, iocp, epoll: contributions welcome

Currently supported architectures:
- amd64 (x86_64)
- arm64 (aarch64)

Current and planned features:
- [x] custom async desugaring
- [x] low latency and zero overhead read/write APIs with the same ergonomics as `std::io`, now with 100% less allocations, `thread_local` lookups, memory copy, and passing owned buffers
- [x] dynamic dispatch
- [x] low overhead spawn, select, join
- [x] safe async closures, async fn mut
- [x] use sync code in an async manner, without rewriting existing sync code
- [ ] multithreading (in progress)
- [ ] async drop (compiler support is ideal)
- [ ] io_uring goodies (fixed file, buffer select)