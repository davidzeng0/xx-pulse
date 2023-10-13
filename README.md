# xx-pulse
A zero[^1] overhead library for asynchronous tasks, using [fibers](https://github.com/davidzeng0/xx-core/blob/main/src/coroutines/README.md).

Available I/O Backends:
- io_uring

## Note: this library is currently not unwind safe.
Catching an unwind may lead to memory leaks.

[^1]: Overhead is around ~4% (60ns) per one send/recv for a simple TCP echo server built with lto, calculated via `usr / (sys + usr)`<br>[xe](https://github.com/davidzeng0/xe) is ~1% (15ns), [tokio](https://github.com/tokio-rs/tokio) (single thread, lto) at ~8% (120ns), tested on Linux kernel 6.4 on AMD 5800X CPU