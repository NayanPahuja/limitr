# limitr: A Distributed Rate Limiting Service

## Overview

**limitr** is a distributed rate limiting service built in Rust. It provides a gRPC interface and uses Redis/Valkey as a shared backend to enforce complex, multi-rate limiting policies across horizontally scaled services. The core logic is based on a multi-rate leaky bucket algorithm, ensuring smooth traffic shaping and reliable protection for downstream systems.

## Features

* **Async and Efficient**: Uses the Tokio async runtime with an optimized Redis Lua script for atomic operations.
* **Distributed & Scalable**: Stateless by design, enabling horizontal scaling. All rate limit state is managed in Redis for consistency across instances.
* **Multi-Rate Policies**: Supports multiple concurrent rate limits (e.g., per-second, per-minute, per-hour) for a single key.
* **Hot Reloading**: Updates rate limit configurations atomically from file without restarting the service.
* **High Availability**: Employs a "fail-open" strategy—requests pass through if Redis becomes unavailable.

## Architecture

The service processes gRPC requests using a Tonic server. Each request checks an in-memory configuration cache to determine applicable rate limits, then executes a Redis Lua script via a connection pool to update state and decide if the request is allowed.

A background watcher monitors the configuration file for changes, validates the new rules, and atomically swaps in the updated configuration. This ensures no interruption for in-flight requests.

## Technology Stack

| Library | Purpose |
| :--- | :--- |
| `tonic` | gRPC server and client framework |
| `prost` | Protocol Buffers code generation |
| `tokio` | Asynchronous runtime |
| `redis-rs` | Redis client library |
| `deadpool-redis` | Connection pooling for Redis |
| `arc-swap` | Atomic, lock-free swapping of configuration |
| `notify` | Watches configuration file changes |
| `serde` | Serialization and deserialization |
| `tracing` | Structured logging and diagnostics |
| `thiserror` | Error handling and reporting |

## Directory Structure

```
📦 limitr
├─ .gitignore
├─ Cargo.toml
├─ LICENSE
├─ README.md
├─ build.rs
├─ example
│  └─ config
│     └─ rate_limit_config.json
├─ proto
│  └─ limitr
│     └─ v1
│        └─ limitr.proto
├─ rust-rate-limiter-trd.md
├─ scripts
│  └─ leaky_bucket.lua
└─ src
   ├─ config
   │  ├─ loader.rs
   │  ├─ mod.rs
   │  ├─ validator.rs
   │  └─ watcher.rs
   ├─ errors.rs
   ├─ lib.rs
   ├─ limiter
   │  ├─ leaky_bucket.rs
   │  └─ mod.rs
   ├─ main.rs
   ├─ redis
   │  ├─ client.rs
   │  ├─ mod.rs
   │  ├─ pool.rs
   │  └─ script.rs
   └─ server
      ├─ handler.rs
      └─ mod.rs
```

## How to Use

### Build from Source

**Prerequisites**  
* Rust toolchain (v1.70+)  
* Running Redis or Valkey instance  

1. Clone the repository:

   ```bash
   git clone https://github.com/your-repo/limitr.git
   cd limitr
   ```

2. Set configuration and environment variables:

   ```bash
   export RATE_LIMIT_CONFIG=./config/rate_limits.json
   export REDIS_CLUSTER_URL=redis://127.0.0.1:6379
   export GRPC_PORT=50051
   ```

3. Build and run:

   ```bash
   cargo run --release
   ```

   The gRPC server starts on the configured port.

*(Docker support planned for a later milestone.)*

## Algorithm Reference

The rate limiting logic follows the **Multi-Rate Leaky Bucket Algorithm** (Microsoft’s implementation), enforcing multiple limits per key while storing state efficiently in Redis using MessagePack serialization.

## Project Milestones

* [x] Setup project and dependencies  
* [x] Setup Proto definitions and build  
* [x] Define standard errors  
* [x] Implement gRPC server and health check  
* [x] Implement config modules and validation  
* [x] Implement Redis layer and test connection  
* [x] Implement limiter logic  
* [x] Add configuration hot reloading  
* [ ] Add Prometheus metrics

## License

This project is licensed under the **Apache License V2**.  
See the [LICENSE](./LICENSE) file for details.
````
