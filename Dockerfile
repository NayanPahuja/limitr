FROM rust:1.90.0-bullseye AS builder

WORKDIR /app

# Install minimal build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends curl unzip \
  && curl -L -o /tmp/protoc.zip https://github.com/protocolbuffers/protobuf/releases/download/v3.20.1/protoc-3.20.1-linux-x86_64.zip \
  && unzip /tmp/protoc.zip -d /usr/local \
  && rm /tmp/protoc.zip \
  && apt-get remove -y curl unzip \
  && rm -rf /var/lib/apt/lists/*

# Copy manifest files first to take advantage of Docker layer caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy src/main.rs so Cargo will download dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Fetch dependencies (speeds up rebuilds when only source changes)
RUN cargo fetch

# Copy proto/build files early if your build.rs or prost needs them
COPY proto ./proto
COPY build.rs ./

# Copy the source tree
COPY src ./src
COPY scripts ./scripts

# Build the release binary
RUN cargo build --release


# ---------- Stage 2: Runtime (slim Debian Bullseye) ----------
FROM debian:bullseye-slim

# Minimal runtime deps (ca-certificates to allow TLS to work)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/limitr-server /usr/local/bin/limitr-server

EXPOSE 50051

ENTRYPOINT ["/usr/local/bin/limitr-server"]
