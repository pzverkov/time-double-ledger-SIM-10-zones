FROM rust:1.93-bookworm AS build
# Set REDPANDA=1 to compile the Kafka-API broker (links librdkafka via cmake).
# Default build stays lean and pulls none of this.
ARG REDPANDA=0
WORKDIR /src
RUN if [ "$REDPANDA" = "1" ]; then \
      apt-get update && \
      apt-get install -y --no-install-recommends cmake build-essential pkg-config && \
      rm -rf /var/lib/apt/lists/*; \
    fi
COPY Cargo.toml ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs
RUN if [ "$REDPANDA" = "1" ]; then cargo build --release --features redpanda; else cargo build --release; fi
COPY . .
RUN if [ "$REDPANDA" = "1" ]; then cargo build --release --features redpanda; else cargo build --release; fi

FROM gcr.io/distroless/cc:nonroot
COPY --from=build /src/target/release/time-ledger-sim-rust /usr/local/bin/sim-rust
USER nonroot:nonroot
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/sim-rust"]
