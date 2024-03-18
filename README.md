# xx-pulse
The fastest async runtime for rust

Available I/O Backends:
- io_uring (requires linux kernel version >= 5.6, recommended 5.11 or 6.1 for best performance)
- kqueue, iocp, epoll: contributions welcome

### Note:
This library is currently only available for Linux (contributions welcome).<br>
For Windows and Mac users, running in Docker or WSL also work.