FROM rust:stable as build
WORKDIR /src
COPY Cargo.toml ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs
RUN cargo build --release
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/time-ledger-sim-rust /usr/local/bin/sim-rust
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/sim-rust"]
