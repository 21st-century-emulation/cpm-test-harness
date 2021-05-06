FROM rustlang/rust:nightly as builder

RUN USER=root cargo new --bin cpm-test-harness
WORKDIR /cpm-test-harness
COPY ./Cargo.lock ./Cargo.toml ./
RUN cargo build --release
RUN rm src/*.rs
COPY ./src ./src
RUN rm -f ./target/release/deps/cpm_test_harness*
RUN cargo build --release

FROM ubuntu:20.04

RUN apt update && apt install -y libssl-dev

COPY --from=builder /cpm-test-harness/target/release/cpm-test-harness .
EXPOSE 8080
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8080
ENV ROCKET_ENV=production
ENTRYPOINT ["./cpm-test-harness"]