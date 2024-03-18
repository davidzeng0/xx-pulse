FROM rust:1.76.0

RUN apt update && \
	apt install -y valgrind && \
	apt clean

RUN cargo install cargo-valgrind cargo-llvm-cov
RUN rustup default nightly-2024-02-10 && \
	rustup component add clippy && \
	rustup default stable

WORKDIR /base
ENTRYPOINT [ "/bin/bash", "-c" ]