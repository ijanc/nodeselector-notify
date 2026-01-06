FROM rust:1.83 AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/target/release/nodeselector-notify /nodeselector-notify

USER 65534:65534

ENTRYPOINT ["/nodeselector-notify"]
