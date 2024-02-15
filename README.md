# xx-pulse
The fastest async runtime for rust

Available I/O Backends:
- io_uring (requires kernel version >= 5.6, recommended 6.1 or 5.11)
- kqueue, iocp, epoll: contributions welcome

## Note: this library is not verified for unwind safety.
Catching an unwind may lead to memory leaks.
