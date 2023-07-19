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

RUN echo 'swagger_url = "http://127.0.0.1:8888"' >>/service/config.toml
RUN echo 'listen_address = "0.0.0.0"' >>/service/config.toml
RUN echo 'listen_port = 8888' >>/service/config.toml
RUN echo 'findora_query_url = "http://127.0.0.1:8668"' >>/service/config.toml
RUN echo 'web3_http_url = "http://127.0.0.1:8545"' >>/service/config.toml
RUN echo 'contract_address = "0xbD694Bf489eE062d0b18da456177Ba623dcDEbF9"' >>/service/config.toml
RUN echo 'dir_path = "/data"' >>/service/config.toml

ENV CONFIG_FILE_PATH=/service/config.toml
ENTRYPOINT ["/service/nft-issue-transaction"]
