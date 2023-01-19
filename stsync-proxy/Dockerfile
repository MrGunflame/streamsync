FROM rust:alpine as builder

RUN apk add musl-dev g++ openssl-dev make
WORKDIR /app
COPY . .

RUN cd stsync-proxy && make build

FROM scratch

WORKDIR /

COPY --from=builder /app/target/release/stsync-proxy /streamsync

ENTRYPOINT ["/streamsync"]
