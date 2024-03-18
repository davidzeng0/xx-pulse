FROM rust:1.76.0

RUN apt update && \
	apt install -y valgrind && \
	apt clean

RUN cargo install cargo-valgrind cargo-llvm-cov
RUN rustup install nightly
RUN rustup component add clippy

WORKDIR /base
ENTRYPOINT [ "/bin/bash", "-c" ]