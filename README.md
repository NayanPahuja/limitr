
-----
# limitr: A Distributed Rate Limiting Service

## Overview

**limitr** is a distributed, rate limiting service built in Rust. It provides a gRPC interface and uses Redis/Valkey as a shared backend to enforce complex, multi-rate limiting policies across horizontally scaled services. The core logic is based on a multi-rate leaky bucket algorithm, ensuring smooth traffic shaping and robust protection for downstream systems.

## Features

  * **High Performance**: Built on the Tokio async runtime with an optimized Redis Lua script for atomic operations. Aims for sub-10ms latency per check.
  * **Distributed & Scalable**: A stateless design allows for seamless horizontal scaling. All state is managed centrally in Redis, enabling consistency across any number of service instances.
  * **Advanced Rate Limiting**: Implements a multi-rate leaky bucket algorithm, allowing for the simultaneous enforcement of multiple policies (e.g., per-second, per-minute, and per-hour limits) on a single key.
  * **Configuration Hot-Reloading**: Atomically swaps in new rate limiting rules from a configuration file on-the-fly without service restarts or downtime.
  * **High Availability**: Employs a "fail-open" strategy by default. If the Redis backend is unavailable, requests are allowed to pass, prioritizing service availability over strict enforcement.

## Architecture

The service operates on a layered architecture. An incoming gRPC request is handled by a Tonic server, which consults an in-memory configuration cache to retrieve the relevant rate limit policies. It then uses a Redis connection pool to execute an atomic Lua script that updates the bucket state and determines if the request should be allowed.

A background task watches the configuration file for changes. Upon modification, it validates and loads the new rules into a new cache instance, which is then atomically swapped with the old one, ensuring all new requests use the updated configuration without interrupting in-flight requests.

## Technology Stack

| Library | Purpose |
| :--- | :--- |
| `tonic` | gRPC server and client framework. |
| `prost` | Protocol Buffers code generation. |
| `tokio` | Asynchronous runtime for concurrent operations. |
| `redis-rs` | The primary client for Redis communication. |
| `deadpool-redis` | Asynchronous connection pooling for Redis. |
| `arc-swap` | Atomic, lock-free swapping of `Arc` pointers for hot-reloading. |
| `notify` | File system watching for configuration changes. |
| `serde` | Framework for serializing and deserializing Rust data structures. |
| `tracing` | A framework for instrumenting Rust programs to collect structured, event-based diagnostic information. |
| `thiserror` | A library for deriving `std::error::Error` implementations. |

## How to Use

### Build from Source

**Prerequisites**:

  * Rust toolchain (v1.70+)
  * A running Redis or Valkey instance

<!-- end list -->

1.  **Clone the repository:**

    ```bash
    git clone https://github.com/your-repo/limitr.git
    cd limitr
    ```

2.  **Configure the service:**
    Create a `config/rate_limits.json` file for your policies. Set the required environment variables, primarily `REDIS_CLUSTER_URL`.

    ```bash
    export RATE_LIMIT_CONFIG=./config/rate_limits.json
    export REDIS_CLUSTER_URL=redis://127.0.0.1:6379
    export GRPC_PORT=50051
    ```

3.  **Build and run the service:**

    ```bash
    cargo run --release
    ```

    The gRPC server will start on the configured host and port.

*(Docker support will be added in a future milestone for easier deployment.)*

## References

The core rate limiting logic is an implementation of the **Multi-Rate Leaky Bucket Algorithm** as described by Microsoft, which allows for enforcing multiple leak rates and burst capacities in parallel for a single key. The state is efficiently stored in Redis using MessagePack for binary serialization.

## Project Milestones

  * [x] **1. Setup Project with cargo workspace and add Dependencies**
  * [x] **2. Setup Proto and build them**
  * [x] **3. Define Standard Errors as decided**
  * [x] **4. Setup a grpc server that works with proto and exposes decided methods and a health check method**
  * [x] **5. Define configs (mod.rs) for config, limiter and redis modules**
  * [x] **6. Config Parsing from file with validation**
  * [x] **7. Define traits for all modules**
  * [x] **8. Write redis layer functionality module and test redis connection script**
  * [x] **9. Write limiter layer functionality module**
  * [x] **10. Hot reloading for config**
  * [ ] **11. Metrics Emission (prometheus standard)**