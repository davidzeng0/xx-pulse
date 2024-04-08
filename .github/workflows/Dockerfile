FROM rust:1.77.1

RUN apt update && \
	apt install -y valgrind && \
	apt clean

RUN cargo install cargo-valgrind cargo-llvm-cov

WORKDIR /base
ENTRYPOINT [ "/bin/bash", "-c" ]