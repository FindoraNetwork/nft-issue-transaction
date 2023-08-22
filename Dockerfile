FROM clux/muslrust AS builder

ENV OPENSSL_DIR /musl
RUN rustup target add x86_64-unknown-linux-musl

COPY . /nft-issue-transaction
WORKDIR /nft-issue-transaction
RUN cargo build --release --target x86_64-unknown-linux-musl

RUN mkdir /binaries
RUN cp target/x86_64-unknown-linux-musl/release/nft-issue-transaction /binaries

RUN strip --strip-all /binaries/nft-issue-transaction
 
FROM alpine:latest
RUN apk --no-cache add ca-certificates

RUN mkdir /service
WORKDIR /service

COPY --from=builder /binaries/nft-issue-transaction /service/nft-issue-transaction

ENV CONFIG_FILE_PATH=/service/config.toml
ENTRYPOINT ["/service/nft-issue-transaction"]
