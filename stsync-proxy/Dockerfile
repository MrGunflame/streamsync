FROM rust:alpine as builder

RUN apk add musl-dev g++ openssl-dev
WORKDIR /app
COPY . .

RUN cargo build --bin stsync-proxy --release

FROM scratch

WORKDIR /

COPY --from=builder /app/target/release/stsync-proxy /bin

ENTRYPOINT ["/bin"]
