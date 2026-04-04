FROM rust:1.93-bookworm AS build
WORKDIR /src
COPY Cargo.toml ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs
RUN cargo build --release
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc:nonroot
COPY --from=build /src/target/release/time-ledger-sim-rust /usr/local/bin/sim-rust
USER nonroot:nonroot
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/sim-rust"]
