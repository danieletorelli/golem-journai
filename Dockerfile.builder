FROM rust:latest AS builder

RUN apt-get update && apt-get install -y curl unzip

WORKDIR /usr/src

RUN curl -Ssf -L -o golem.zip https://github.com/golemcloud/golem/archive/refs/heads/main.zip && \
    unzip golem.zip && \
    mv golem-main golem

RUN cd /usr/src/golem/cli/golem && \
    cargo build --release && \
    mv /usr/src/golem/target/release/golem /usr/local/bin && \
    chmod +x /usr/local/bin/golem

FROM ubuntu:latest

RUN apt-get update && apt-get install -y openssl

COPY --from=builder /usr/local/bin/golem /usr/local/bin/golem

ENTRYPOINT ["golem", "server", "run", "-vv"]
