# coverage twin of the pinned stellar-core release. twin/build.sh drives both
# targets; building this file by hand is not supported, because the compile
# itself runs in a `docker run` with memory and cpu limits (docker build has no
# memory limit under buildkit) against a source tree kept on disk.
#
#   toolchain  ubuntu noble with the compiler set core's own Dockerfile.testing
#              uses (clang-20, libc++-20) plus llvm-20 for the coverage tools,
#              and the rust toolchain core pins in rust-toolchain.toml,
#              installed by core's install-rust.sh (pinned rustup, checked sha).
#   runtime    the built stellar-core binary on the same base, with the shared
#              libraries it links, llvm-profdata / llvm-cov to read the
#              profiles it writes, and python3, which shapekit's driver runs
#              inside the container as its http client (the release image
#              ships it too).

ARG BASE

FROM ${BASE} AS toolchain
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        ca-certificates git build-essential pkg-config autoconf automake libtool \
        bison flex sed perl libpq-dev parallel curl \
        clang-20 llvm-20 libc++abi-20-dev libc++-20-dev libclang-rt-20-dev && \
    rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rust/rustup CARGO_HOME=/opt/rust/cargo
ENV PATH=/opt/rust/cargo/bin:$PATH
COPY install-rust.sh rust-toolchain.toml /opt/rust-install/
# the build runs as the invoking host user, so the toolchain must be usable
# (and rustup's state writable) by any uid
RUN cd /opt/rust-install && sh ./install-rust.sh && chmod -R a+rwX /opt/rust

FROM ${BASE} AS runtime
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        ca-certificates libpq5 libc++1-20 libc++abi1-20 llvm-20 python3 && \
    rm -rf /var/lib/apt/lists/*
COPY stellar-core /usr/local/bin/stellar-core
COPY build-info.txt /usr/local/share/stellar-core-twin/build-info.txt
WORKDIR /etc/stellar
ENTRYPOINT ["/usr/local/bin/stellar-core"]
