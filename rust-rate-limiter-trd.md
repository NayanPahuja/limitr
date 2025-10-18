# Rust gRPC Rate Limiting Service - Technical Design Document



Author: Nayan Pahuja (Claude tbh)



Date: 18 Oct, 2025



## 1. Overall Summary



This document provides a comprehensive technical specification for building a production-grade rate limiting service in Rust. The service exposes a gRPC interface for language-agnostic client integration and implements domain-based, configurable rate limiting using a leaky bucket algorithm with Redis/Valkey as the distributed storage backend.



## 2. Architecture Overview



### 2.1 System Components



The system consists of five primary layers:



1. **gRPC Interface Layer**: Handles client communication via Protocol Buffers

2. **Business Logic Layer**: Implements rate limiting decisions and configuration management.

3. **Storage Layer**: Redis/Valkey distributed state management using lua script.

4. **Configuration Layer**: Dynamic configuration loading and watching

5. **Connection Pool Layer**: Redis connection management and failover



### 2.2 Core Design Principles



- **Distributed by design**: Horizontal scalability through shared Redis state

- **Atomic operations**: Leverage Redis Lua scripts for consistency

- **Graceful degradation**: Fail-open strategy when Redis is unavailable

- **Memory safety**: Leverage Rust’s type system to prevent common memory errors

- **Trait Logic**: Leverage Rust’s trait system to prevent a common interface for all algorithms.

- **Connection efficiency**: Connection pooling and pipelining for throughput

- **Observability**: Comprehensive metrics and tracing



## 3. Technology Stack



### 3.1 Primary Dependencies



| Library | Version | Purpose |

|---------|---------|---------|

| `tonic` | 0.11+ | gRPC server implementation |

| `prost` | 0.12+ | Protocol Buffers code generation |

| `tokio` | 1.35+ | Async runtime |

| `redis` | 0.24+ | Redis client with async support |

| `fred` | 8.0+ | Alternative high-performance Redis client |

| `rmp-serde` | 1.1+ | MessagePack serialization for Rust |

| `serde` | 1.0+ | Configuration serialization |

| `serde_json` | 1.0+ | JSON configuration parsing |

| `notify` | 6.1+ | File system watching for config changes |

| `tracing` | 0.1+ | Structured logging |

| `tracing-subscriber` | 0.3+ | Log collection and formatting |

| `thiserror` | 1.0+ | Error type derivation |

| `deadpool-redis` | 0.14+ | Redis connection pooling |



### 3.2 Build Dependencies



- `tonic-build` 0.11+: For compiling .proto files during build

- `prost-build` 0.12+: Protocol Buffers compiler integration



## 4. gRPC Interface Specification



### 4.1 Protocol Buffer Definition



```protobuf

syntax = "proto3";



package ratelimiter.v1;



service RateLimiterService {

  rpc ConsumeAndCheckLimit(CheckRequest) returns (CheckResponse);

  rpc GetCurrentConfig(ConfigRequest) returns (ConfigResponse);

  rpc GetBucketStatus(StatusRequest) returns (StatusResponse);

}



message CheckRequest {

  optional string domain = 1;

  string limit_key = 2;

  optional int32 cost = 3; // Number of tokens to consume (default: 1)

}



message CheckResponse {

  bool allowed = 1;

  double remaining_capacity = 2; // Can be negative if denied

  int32 limiting_rate_index = 3; // Which rate policy was most restrictive

  int64 deny_count = 4; // Total tokens denied (only when allowed=false)

}



message ConfigRequest {}



message ConfigResponse {

  repeated DomainConfig configs = 1;

}



message DomainConfig {

  string domain = 1;

  string prefix_key = 2;

  repeated RatePolicy policies = 3; // Multiple rate policies (flow_rate, burst)

}



message RatePolicy {

  double flow_rate_per_second = 1; // Leak rate (tokens per second)

  int64 burst_capacity = 2;         // Maximum bucket capacity

  string name = 3;                  // Optional name (e.g., "per_second", "per_minute")

}



message StatusRequest {

  optional string domain = 1;

  string limit_key = 2;

}



message StatusResponse {

  repeated BucketLevel levels = 1;

  int64 last_update_timestamp = 2;

  int64 deny_count = 3;

}



message BucketLevel {

  double current_level = 1;

  double flow_rate = 2;

  int64 burst_capacity = 3;

  double remaining_capacity = 4;

}

```



### 4.2 API Semantics



### ConsumeAndCheckLimit



- **Purpose**: Attempts to add tokens to the bucket and checks if request is allowed

- **Behavior**:

    - If domain is not specified or if there is no config given. Load default config written in code as a constant.

    - Leaks tokens based on elapsed time since last access.

    - Adds requested tokens if bucket has capacity

    - Returns allowance status with bucket metadata

- **Thread Safety**: Redis Lua scripts ensure atomicity

- **Idempotency**: Not idempotent - each call consumes capacity



### GetCurrentConfig



 **Purpose**: Returns current active configuration for observability

- **Use Cases**: Debugging, monitoring, configuration verification

- **Performance**: Read-only operation from local cache



#### GetBucketStatus

-- **Purpose**: Returns current bucket state without consuming tokens

- **Use Cases**: Monitoring, debugging, capacity planning

- **Performance**: Single Redis GET operation



## 5. Rate Limiting Algorithm

### 5.1 Leaky Bucket Algorithm (Multi-Rate)

The multi-rate leaky bucket algorithm models rate limiting as multiple buckets with different capacities and leak rates operating in parallel. All buckets must have sufficient capacity for a request to be allowed. This implements the Microsoft approach for complex rate limiting scenarios.

#### Algorithm Characteristics

**Advantages:**
- **Multi-tier protection**: Enforce multiple rate limits simultaneously (e.g., per-second AND per-minute)
- Smooth traffic flow with constant output rate per policy
- Natural burst handling up to each bucket's capacity
- Protects downstream services with predictable load
- Simple mental model: multiple leaking buckets in parallel
- Avoids boundary burst issues of fixed windows
- More flexible than single-rate limiting

**Disadvantages:**
- More complex state management (multiple token levels)
- Higher memory usage (store level for each policy)
- Requires timestamp tracking for leak calculations
- Higher computational cost per request (multiple calculations)
- More complex configuration and tuning

#### Implementation Strategy

1. **State Storage**: Store (last_time, deny_count, token_levels[]) in Redis using MessagePack binary format
2. **Leak Calculation**: For each policy: `leaked = (now - last_time) * flow_rate`
3. **New Levels**: For each policy: `current_level = max(0, old_level - leaked)`
4. **Token Addition**: Add cost to all bucket levels
5. **Limit Check**: Request allowed only if ALL buckets have capacity (remaining >= 0)
6. **Atomic Updates**: Use Redis Lua script for read-modify-write
7. **Binary Packing**: Use cmsgpack for efficient state serialization

### 5.2 Multi-Rate Leaky Bucket Mathematics

```
Core Equations (per policy i):
- flow_rate[i] = configured rate (tokens per second)
- burst_capacity[i] = maximum bucket size (tokens)
- time_delta = current_time - last_update_time (seconds)
- tokens_leaked[i] = flow_rate[i] × time_delta
- new_level[i] = max(0, old_level[i] - tokens_leaked[i])
- next_level[i] = new_level[i] + cost
- remaining[i] = burst_capacity[i] - next_level[i]

Multi-Rate Decision Logic:
- free_capacity = min(remaining[0], remaining[1], ..., remaining[n])
- limiting_index = argmin(remaining[i]) // which policy is most restrictive
- allowed = (free_capacity >= 0)

Deny Count Tracking:
- If denied: deny_count += cost
- If allowed: deny_count = 0
- Useful for monitoring persistent violations

TTL Calculation:
- For each policy: ttl[i] = ceil(max(burst_capacity[i], next_level[i]) / flow_rate[i])
- expire_time = max(ttl[0], ttl[1], ..., ttl[n])
- Ensures key doesn't expire while buckets still have tokens

Example Multi-Rate Configuration:
Policy 1: flow_rate=10/sec, burst=100 (short-term limit)
Policy 2: flow_rate=1000/min≈16.67/sec, burst=1000 (per-minute limit)
Policy 3: flow_rate=10000/hour≈2.78/sec, burst=10000 (hourly limit)

Request Allowed If:
- Remaining capacity in 10/sec bucket >= 0 AND
- Remaining capacity in 1000/min bucket >= 0 AND
- Remaining capacity in 10000/hour bucket >= 0
```

### 5.3 Redis Lua Script Implementation (Microsoft Multi-Rate)

The core algorithm uses the Microsoft multi-rate implementation with binary packing via MessagePack:

```lua
--[[
Leaky Bucket (Multi-Rate) Algorithm — Microsoft Version
------------------------------------------------------
Implements a rate limiter supporting multiple leak rates and burst capacities.
Uses Redis as backend, storing state as packed binary (cmsgpack).
KEYS[1] - Redis key representing the bucket
ARGV[1] - cost (tokens requested for this operation)
ARGV[2..n] - pairs of [flow_rate, burst_capacity] for each rate policy
--]]

-- Enable command replication for Redis Cluster / replicas
redis.replicate_commands()

-- Read inputs
local key = KEYS[1]
local cost = tonumber(ARGV[1]) -- tokens requested (usually 1)

-- Get Redis server time (seconds + microseconds)
local raw_time = redis.call('time')
local now = tonumber(raw_time[1]) + tonumber(raw_time[2]) / 1e6 -- high precision current time

-- Try to read current bucket state (packed binary data)
-- Expected packed data: { last_time, deny_count, token_levels[] }
local okay, last_time, deny_count, token_levels =
    pcall(cmsgpack.unpack, redis.pcall('get', key))

-- If unpacking failed or key doesn't exist, initialize new bucket
if not okay then
    last_time = now
    deny_count = 0
    token_levels = {}
end

-- Compute time delta since last update (seconds)
local delta = now - last_time

-- Prepare next-state variables
local next_levels = {}        -- token levels after adding cost
local expire_time = 0         -- how long the key should live (seconds)
local free_capacity = math.huge  -- smallest remaining capacity among all limits
local limiting_index = 0         -- which limit was most restrictive

-- Loop through each (flow_rate, burst_capacity) pair
for i = 1, #ARGV / 2 do
    local flow = tonumber(ARGV[2 * i])       -- leak rate (tokens per second)
    local burst = tonumber(ARGV[2 * i + 1])  -- max capacity for this rate

    -- Leak tokens that should have leaked since last update
    local previous_level = token_levels[i] or 0
    local leaked_level = math.max(0, previous_level - (delta * flow))

    -- Add requested cost (tokens)
    next_levels[i] = leaked_level + cost

    -- Calculate how much space remains in this bucket
    local remaining = burst - next_levels[i]

    -- Track the smallest remaining capacity (the limiting rate)
    if remaining < free_capacity then
        free_capacity = remaining
        limiting_index = i
    end

    -- Estimate TTL: how long until this bucket fully leaks
    expire_time = math.max(expire_time, math.ceil(math.max(burst, next_levels[i]) / flow))
end

-- Decide whether to allow or deny this operation
if free_capacity >= 0 then
    -- ✅ Allowed request:
    -- Store updated state: (time, deny_count, new_levels)
    redis.call('setex', key, expire_time, cmsgpack.pack(now, 0, next_levels))

    -- Return success, remaining capacity, and which limit applied
    return { 1, tostring(free_capacity), limiting_index }
else
    -- ❌ Denied request:
    deny_count = deny_count + cost

    -- Keep the previous levels but update deny count and timestamp
    redis.call('setex', key, expire_time, cmsgpack.pack(now, deny_count, token_levels))

    -- Return failure, total denied tokens, and limiting rate index
    return { 0, tostring(deny_count), limiting_index }
end
```

**Key Features:**
1. **MessagePack Serialization**: Compact binary storage of state
2. **Redis TIME Command**: High-precision timestamps avoid clock skew
3. **Command Replication**: Ensures script works with Redis Cluster
4. **Multiple Policies**: Each with independent flow_rate and burst_capacity
5. **Limiting Index**: Identifies which policy is most restrictive
6. **Deny Counter**: Tracks accumulated denied tokens for monitoring

**Return Values:**
- `[1, remaining_capacity, limiting_index]` - Allowed
- `[0, deny_count, limiting_index]` - Denied

### 5.4 Key Construction

```rust
fn construct_bucket_key(domain: &str, prefix: &str, limit_key: &str) -> String {
    format!("bucket:{}:{}:{}", domain, prefix, limit_key)
}

```

## 6. Redis Architecture

### 6.1 Redis/Valkey Selection

**Primary Choice: Valkey**
- Open-source Redis fork with active development
- API-compatible with Redis
- Better licensing for production use
- Active community support

**Fallback: Redis OSS**
- Well-established and stable
- Wide tooling support
- Compatible client libraries

**Deployment Modes:**
1. **Standalone**: Single instance for development (for now v0)


### 6.2 Data Model

#### Bucket State Storage (MessagePack Binary)

```
Key: bucket:{domain}:{prefix}:{limit_key}
Type: String (MessagePack-encoded binary)
Value: Packed array [last_time, deny_count, token_levels[]]
  - last_time: float (Unix timestamp with microseconds)
  - deny_count: integer (accumulated denied tokens)
  - token_levels: array of floats (one per rate policy)
TTL: max(ceil(burst[i] / flow[i])) across all policies

Example (unpacked representation):
Key: "bucket:api.example.com:user:user123"
Value: [1729267200.123456, 0, [45.5, 890.2, 5670.8]]
  // 3 rate policies with current levels: 45.5, 890.2, 5670.8
TTL: 3600 seconds
```

**Why MessagePack:**
- Compact binary format (~40% smaller than JSON)
- Native Lua support via `cmsgpack`
- Efficient number encoding (no string conversion)
- Preserves floating-point precision
- Fast serialization/deserialization

#### Configuration Cache (Optional)

```
Key: config:{domain}:{prefix}
Type: Hash
Fields:
  - capacity: Bucket capacity
  - leak_rate: Tokens per second
  - updated_at: Last config update timestamp
TTL: 300 seconds (5 minutes)
```

### 6.3 Connection Pool Configuration

**Pool Settings:**
```rust
deadpool_redis::Config {
    max_size: 50,              // Maximum connections
    timeouts: Timeouts {
        wait: Some(Duration::from_secs(5)),
        create: Some(Duration::from_secs(5)),
        recycle: Some(Duration::from_secs(5)),
    },
    // Sentinel configuration for HA
    sentinel: Some(SentinelConfig {
        master_name: "mymaster".to_string(),
        sentinels: vec![
            "sentinel1:26379",
            "sentinel2:26379",
            "sentinel3:26379",
        ],
    }),
}
```

**Connection Lifecycle:**
- **Acquisition**: Get from pool with timeout
- **Health Check**: Ping before use if idle > 30s
- **Error Handling**: Return broken connections to pool for cleanup
- **Recycling**: Close connections older than 1 hour

### 6.4 Redis Cluster Considerations

**Hash Slot Distribution:**
- All keys for a bucket use the same prefix
- Use Redis hash tags: `{bucket:domain:prefix}:limit_key`
- Ensures atomic operations on same slot

**Multi-key Operations:**
- Lua scripts require all keys on same node
- Use hash tags to colocate related keys
- Alternative: Fan-out to multiple scripts


## 7. Memory Management in Redis

### 7.1 TTL-Based Expiration

**Automatic Cleanup:**
- Every bucket key has TTL set after update
- TTL = 2 × (capacity / leak_rate) for safety margin
- Redis automatically removes expired keys
- No manual cleanup required

**TTL Calculation Examples:**
```
capacity=1000, leak_rate=10/sec → TTL = 200 seconds
capacity=100, leak_rate=1/sec → TTL = 200 seconds
capacity=3600, leak_rate=1/sec → TTL = 7200 seconds (2 hours)
```

### 7.2 Redis Memory Policies

**Recommended: `volatile-lru`**
- Evicts keys with TTL using LRU when memory limit reached
- Protects keys without TTL
- Appropriate for rate limiting use case


### 7.3 Memory Estimation

**Per Bucket Overhead:**
```
Key size: ~60 bytes (bucket:domain:prefix:limit_key)
Hash overhead: ~50 bytes
Fields: 2 × (field_name + value) = ~40 bytes
Total per bucket: ~150 bytes

For 1M active buckets: ~150 MB
For 10M active buckets: ~1.5 GB
```

**Redis Memory Overhead:**
- Internal data structures: ~20% additional
- Replication buffer: Configurable, default 64MB
- AOF buffer: If persistence enabled


## 8. Configuration Management

### 8.1 Configuration Schema

```json
{
ains": [
    {
      "domain": "api.example.com",
      "prefix": "user",
      "rate": {
        "capacity": 100,
        "leak_rate_per_second": 10.0
      }
    },
    {
      "domain": "api.example.com",
      "prefix": "admin",
      "rate": {
        "capacity": 1000,
        "leak_rate_per_second": 100.0
      }
    }
  ]
}
```

### 8.2 Configuration Loading

**Redis Config**



- Read this from env and validate:

    - REDIS_CLUSTER_URL

    - REDIS_USE_TLS

    - REDIS_CLUSTER_USERNAME

    - REDIS_CLUSTER_PASSWORD

    - REDIS_MAX_CONN



and appropriately set the configurations for redis.



**RateLimit Config and Bootstrap Process**:
1. Read config path from environment variable `RATE_LIMIT_CONFIG`
2. Parse JSON into strongly-typed Rust structs
3. Validate configuration:
   - Positive capacity and leak rates
4. Build internal lookup structures (DashMap<Domain, Vec<PrefixConfig>>)
5. Initialize Redis connection pool
6. Start file watcher for hot reload

### 8.3 Hot Reload Mechanism

**File Watching**:
- Use `notify` crate to watch config file
- Detect file modification events
- Debounce rapid changes (wait 100ms after last change)

**Reload Process**:
```
On config file change: Use DashMap
1. Read new file contents
2. Parse and validate new config
3. If validation fails:
     Log error, keep old config
     Return
4. Build new lookup structures
6. If Redis unreachable:
     Log error, keep old config
     Return
7. Create new connection pool
8. Atomically swap config using Arc + RwLock
9. Gracefully drain old connection pool
10. Log reload success with config version
```

**Redis Connection Pool Transition**:
```rust
// Graceful pool migration
let new_pool = create_redis_pool(new_config).await?;
let old_pool = {
    let mut config = config_handle.write().await;
    let old_pool = config.redis_pool.clone();
    config.redis_pool = new_pool;
    old_pool
};

// Allow in-flight requests to complete
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(30)).await;
    // old_pool dropped here, connections close
});
```

### 8.4 Default Domain Behavior

- If `domain` field is `None` in request, map to `"default"`

- If no configuration is present use a standard static config established at runtime called default.

## 9. Concurrency and Thread Safety

### 9.1 Threading Model

**Tokio Runtime Configuration**:
- Use multi-threaded runtime
- Thread pool size: Number of CPU cores
- Separate worker threads for gRPC handlers
- Dedicated thread for config file watching
- Connection pool shared across all threads

### 9.2 Lock Hierarchy

To prevent deadlocks, establish lock ordering:

1. **Config RwLock** (outer): Rarely written, frequently read Use DashMap Here.
2. **Redis Connection Pool** (inner): Managed by deadpool, no explicit locking

**Rules**:
- Snapshot config at request start using `Arc::clone()`
- Hold config lock only for reading, not during Redis operations
- Never hold config lock while waiting on Redis I/O

### 9.3 Atomic Operations

**Atomicity Guarantees:**
- Redis Lua scripts execute atomically
- Single Redis instance = single-threaded script execution
- No interleaving of operations within script
- Network failures detected by client timeout

**Script Registration:**
```rust
// Load script once at startup
let script_sha = redis
    .script_load(LEAKY_BUCKET_SCRIPT)
    .await?;

// Execute using SHA for efficiency
let result: Vec<i64> = redis
    .evalsha(script_sha, &[bucket_key], &[now, leak_rate, capacity, tokens, ttl])
    .await?;
```

### 9.4 Race Condition Scenarios

**Scenario 1: Concurrent Requests to Same Bucket**
- **Problem**: Multiple service instances update same bucket
- **Solution**: Redis Lua script atomicity guarantees serialization

**Scenario 2: Config Reload During Request**
- **Problem**: Config changes mid-request processing
- **Solution**: Snapshot config at request start using `Arc::clone()`

**Scenario 3: Redis Failover During Request**
- **Problem**: Master fails, request in-flight
- **Solution**: Client detects error, fails open and emits metrics.


## 10. Error Handling Strategy

### 10.1 Error Types

```rust
enum RateLimitError {
    ConfigurationError(String),
    InvalidDomain(String),
    InvalidRate(String),
    RedisConnectionError(redis::RedisError),
    RedisCommandError(redis::RedisError),
    ScriptExecutionError(String),
    InternalError(String),
}
```

### 10.2 gRPC Error Mapping

| Internal Error | gRPC Status Code |
|----------------|------------------|
| ConfigurationError | INTERNAL |
| InvalidDomain | INVALID_ARGUMENT |
| InvalidRate | INVALID_ARGUMENT |
| RedisConnectionError | UNAVAILABLE |
| RedisCommandError | INTERNAL |
| ScriptExecutionError | INTERNAL |
| InternalError | INTERNAL |

### 10.3 Fail-Open vs Fail-Closed

**Philosophy**: Fail-open for availability

**Scenarios and Handling:**

1. **Redis Unreachable**:
   - Allow all requests
   - Log critical error
   - Emit metric: `redis_failures_total`
   - Alert operations team

2. **Redis Timeout**:
   - Retry once with shorter timeout
   - If still fails, allow request
   - Log warning

3. **Lua Script Error**:
   - Log script output
   - Allow request (likely transient issue)

4. **Config Load Failure**:
   - Use last known good config
   - Allow requests with old config
   - Log error


## 11. Observability and Monitoring

### 11.1 Structured Logging

**Log Levels**:
- **ERROR**: Redis connection failures, script errors, config failures
- **WARN**: Approaching capacity, Redis timeouts
- **INFO**: Config reloads, service lifecycle, Redis reconnections
- **DEBUG**: Per-request decisions (rate limited)
- **TRACE**: Redis command execution, connection pool stats

**Key Log Events**:
- Request allowed/denied with domain, key, and bucket level
- Config reload success/failure
- Redis connection events (connect, disconnect, failover)
- Circuit breaker state transitions
- Lua script execution errors

### 11.2 Metrics

**Essential Metrics**:

1. **Request Metrics**:
   - `requests_allowed_total` (counter)
   - `requests_denied_total` (counter)
   - `request_duration_seconds` (histogram)
   - `tokens_consumed_total` (counter)
   - `deny_count_total` (counter) - tracks accumulated denials

2. **Redis Metrics**:
   - `redis_command_duration_seconds` (histogram)
   - `redis_connection_pool_size` (gauge)
   - `redis_connection_pool_available` (gauge)
   - `redis_errors_total` (counter by error_type)
   - `redis_script_executions_total` (counter)
   - `redis_failovers_total` (counter)

3. **Bucket Metrics**:
   - `bucket_current_level` (gauge, sampled per policy)
   - `bucket_limiting_rate_index` (histogram) - which policy most often limits
   - `bucket_policies_count` (gauge) - number of active policies
   - `remaining_capacity_by_policy` (gauge per policy index)

4. **Config Metrics**:
   - `config_reloads_total` (counter)
   - `config_reload_failures_total` (counter)
   - `active_domains_total` (gauge)
   - `active_policies_per_domain` (histogram)


**Labels/Dimensions**:
- `domain`: Request domain
- `prefix`: Rate limit prefix
- `allowed`: true/false
- `error_type`: Redis error classification
- `redis_node`: For cluster deployments
- `policy_index`: Which rate policy (0, 1, 2...)
- `policy_name`: Human-readable policy name (per_second, per_minute, etc.)

### 11.3 Health Checks

**gRPC Health Protocol**:
Implement standard gRPC health checking service

**Health Criteria**:

- ✓ GRPC Server Up

- ✓ Config loaded successfully

- ✓ Redis reachable.

**Redis Health Check:**
```rust
async fn check_redis_health(pool: &Pool) -> HealthStatus {
    let start = Instant::now();

    match pool.get().await {
        Ok(mut conn) => {
            match redis::cmd("PING").query_async(&mut *conn).await {
                Ok(redis::Value::Status(s)) if s == "PONG" => {
                    let latency = start.elapsed();
                    if latency < Duration::from_millis(2000) {
                        HealthStatus::Healthy
                    } else {
                        HealthStatus::Degraded(format!("High latency: {:?}", latency))
                    }
                }
                _ => HealthStatus::Unhealthy("Invalid PING response".to_string()),
            }
        }
        Err(e) => HealthStatus::Unhealthy(format!("Connection failed: {}", e)),
    }
}
```

## 12. Performance Considerations


**Optimization Techniques**:

1. **Connection Pooling**: Reuse connections across requests
2. **Lua Scripts**: Single round-trip for complex operations
3. **Script Caching**: Use EVALSHA instead of EVAL
4. **Minimize Data Transfer**: Return only necessary fields

### 12.2 Scalability

**Vertical Scaling**:
- Service: Linear scaling with CPU cores (multi-threaded runtime)
- Redis: Scale memory for more active buckets
- Redis: Upgrade to larger instance for more ops/sec

**Horizontal Scaling**:
- Service: Stateless design, add more instances freely
- Load balancer distributes traffic across instances
- All instances share same Redis backend
- No session stickiness required

**Redis Scaling Options**:

1. **Single Instance**: Up to ~100K ops/sec
   - Simplest deployment
   - No sharding complexity
   - Limited by single-threaded Redis

2. **Master-Replica with Read Splitting**:
   - Read-only queries (GetBucketStatus) to replicas
   - Writes to master only
   - 2-3× read throughput
   - Adds replication lag complexity

3. **Redis Cluster**:
   - Horizontal scaling with sharding
   - Multiple masters, each with replicas
   - 100K+ ops/sec per shard
   - Hash tag required for multi-key operations

### 12.3 Bottleneck Analysis

**Potential Bottlenecks**:

1. **Redis Network Latency**:
   - **Problem**: Each request requires Redis round-trip
   - **Mitigation**:
     - Deploy Redis close to service (same AZ/region)
     - Use Redis pipelining for batched operations
     - Consider Redis Cluster for geographic distribution

2. **Redis Single-Threaded Execution**:
   - **Problem**: Lua scripts execute serially
   - **Mitigation**:
     - Optimize script complexity (keep < 1ms)
     - Use Redis Cluster to shard across cores
     - Monitor `instantaneous_ops_per_sec`

3. **Connection Pool Exhaustion**:
   - **Problem**: All connections in use, requests wait
   - **Mitigation**:
     - Increase pool size
     - Reduce operation timeout
     - Add circuit breaker to fail fast

4. **Redis Memory Pressure**:
   - **Problem**: Evictions cause bucket state loss
   - **Mitigation**:
     - Scale Redis memory
     - Monitor eviction rate
     - Alert on unexpected evictions

## 17. Implementation Roadmap



### Phase 1: Core Implementation (Week 1-2)



- [ ]  Project setup with Cargo workspace

- [ ]  Define Protocol Buffers and generate code

- [ ]  Implement leaky bucket Lua script

- [ ]  Redis connection pool setup

- [ ]  Basic gRPC service handlers

- [ ]  JSON config loading



### Phase 2: Redis Integration (Week 3)
- [ ]  Lua script registration and execution

- [ ]  Redis client configuration

- [ ]  Connection pool management

- [ ]  Error handling and retry logic
### Phase 3: Reliability Features (Week 4)
- [ ]  Fail-open logic

- [ ]  Connection health checks

- [ ]  Graceful shutdown handling (if sys kill recieved)

### Phase 4: Observability (Week 5)

- [ ]  Structured logging with tracing

- [ ]  Prometheus metrics collection

- [ ]  Health check endpoint

### Phase 5: Production Readiness (Week 6-7)

- [ ]  Config hot reload with file watching

- [ ]  Load testing and optimization

## 18. Code Organization

### 18.1 Module Structure

```
rate-limiter/
├── Cargo.toml
├── build.rs                    # tonic-build integration
├── proto/
│   └── ratelimiter.proto      # gRPC service definition
├── scripts/
│   └── leaky_bucket.lua       # Redis Lua script
├── src/
│   ├── main.rs                # Service entry point
│   ├── lib.rs                 # Public API exports
│   ├── config/
│   │   ├── mod.rs             # Config module
│   │   ├── loader.rs          # File loading logic
│   │   ├── watcher.rs         # Hot reload implementation
│   │   └── validation.rs      # Config validation
│   ├── redis/
│   │   ├── mod.rs             # Redis module
│   │   ├── pool.rs            # Connection pool management
│   │   ├── script.rs          # Lua script loading
│   │   ├── sentinel.rs        # Sentinel client setup
│   │   └── commands.rs        # Redis command wrappers
│   ├── limiter/
│   │   ├── mod.rs             # Rate limiter trait
│   │   ├── leaky_bucket.rs    # Leaky bucket implementation
│   │   └── circuit_breaker.rs # Circuit breaker
│   ├── server/
│   │   ├── mod.rs             # gRPC server setup
│   │   └── handlers.rs        # RPC method implementations
│   ├── error.rs               # Error types
│   └── metrics.rs             # Metrics collection
```

### 18.2 Trait Design for Extensibility

```rust
#[async_trait]
trait RateLimiter: Send + Sync {
    async fn check_limit(
        &self,
        domain: &str,
        prefix: &str,
        key: &str,
        config: &RateConfig,
        tokens: u32,
    ) -> Result<LimitDecision, RateLimitError>;

    async fn get_bucket_status(
        &self,
        domain: &str,
        prefix: &str,
        key: &str,
    ) -> Result<BucketStatus, RateLimitError>;
}

struct LeakyBucketLimiter {
    redis_pool: deadpool_redis::Pool,
    script_sha: Arc<String>,
    circuit_breaker: Arc<CircuitBreaker>,
}

#[async_trait]
impl RateLimiter for LeakyBucketLimiter {
    async fn check_limit(&self, ...) -> Result<LimitDecision, RateLimitError> {
        // Implementation
    }
}
```

## 19. Key Implementation Details

### 19.1 Lua Script: Multi-Rate Leaky Bucket

The Microsoft implementation is used directly (see Section 5.3 for full script).

**Script Location:** `scripts/leaky_bucket_multirate.lua`

**Key Implementation Notes:**

1. **Binary State Format:**
   ```rust
   // Rust representation of packed state
   struct BucketState {
       last_time: f64,           // Unix timestamp with microseconds
       deny_count: i64,          // Accumulated denied tokens
       token_levels: Vec<f64>,   // One level per policy
   }
   ```

2. **Argument Packing:**
   ```rust
   // Build ARGV for Lua script
   let mut args = vec![cost.to_string()];
   for policy in &policies {
       args.push(policy.flow_rate.to_string());
       args.push(policy.burst_capacity.to_string());
   }
   ```

3. **Return Value Parsing:**
   ```rust
   // Parse script response
   let result: Vec<redis::Value> = script.invoke_async(&mut conn).await?;

   match &result[..] {
       [redis::Value::Int(allowed), redis::Value::Data(capacity_bytes), redis::Value::Int(index)] => {
           let capacity_str = std::str::from_utf8(capacity_bytes)?;
           let remaining = capacity_str.parse::<f64>()?;

           BucketResponse {
               allowed: *allowed == 1,
               remaining_capacity: remaining,
               limiting_rate_index: *index as i32,
               deny_count: if *allowed == 1 { 0 } else { remaining.abs() as i64 },
           }
       }
       _ => Err(RateLimitError::ScriptExecutionError("Invalid response format".into())),
   }
   ```

### 19.2 Rust Redis Client Implementation

```rust
use deadpool_redis::{Config, Pool, Runtime};
use redis::AsyncCommands;

pub struct RedisClient {
    pool: Pool,
    script_sha: String,
}

impl RedisClient {
    pub async fn new(config: &RedisConfig) -> Result<Self> {
        // Create connection pool
        let pool = create_pool(config).await?;

        // Load Lua script
        let script_sha = load_script(&pool).await?;

        Ok(Self { pool, script_sha })
    }

    pub async fn check_leaky_bucket(
        &self,
        key: &str,
        leak_rate: f64,
        capacity: i64,
        tokens: i64,
    ) -> Result<BucketResponse> {
        // Get connection from pool
        let mut conn = self.pool.get().await
            .map_err(|e| RateLimitError::RedisConnectionError(e))?;

        // Current timestamp in milliseconds
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as i64;

        // TTL = 2 × (capacity / leak_rate)
        let ttl = ((2.0 * capacity as f64) / leak_rate) as i64;

        // Execute Lua script
        let result: Vec<i64> = redis::Script::new(&format!("return redis.call('EVALSHA', '{}', 1, KEYS[1], ARGV[1], ARGV[2], ARGV[3], ARGV[4], ARGV[5])",
            self.script_sha))
            .key(key)
            .arg(now)
            .arg(leak_rate)
            .arg(capacity)
            .arg(tokens)
            .arg(ttl)
            .invoke_async(&mut *conn)
            .await
            .map_err(|e| RateLimitError::RedisCommandError(e))?;

        Ok(BucketResponse {
            allowed: result[0] == 1,
            remaining_capacity: result[1],
            retry_after_seconds: result[2] as f64 / 1000.0,
            current_level: result[3],
        })
    }
}

async fn load_script(pool: &Pool) -> Result<String> {
    let script_content = include_str!("../scripts/leaky_bucket_multirate.lua");
    let mut conn = pool.get().await?;

    let sha: String = redis::Script::new(script_content)
        .prepare_invoke()
        .load_async(&mut *conn)
        .await?;

    tracing::info!("Loaded Lua script with SHA: {}", sha);
    Ok(sha)
}

#[derive(Debug, Clone)]
pub struct RatePolicy {
    pub name: String,
    pub flow_rate_per_second: f64,
    pub burst_capacity: i64,
}

#[derive(Debug)]
pub struct BucketResponse {
    pub allowed: bool,
    pub remaining_capacity: f64,  // Can be negative if denied (represents deny_count)
    pub limiting_rate_index: i32,  // 0-indexed, which policy was most restrictive
    pub deny_count: i64,           // Total accumulated denied tokens
}

#[derive(Debug)]
pub struct BucketState {
    pub last_time: f64,
    pub deny_count: i64,
    pub token_levels: Vec<f64>,
}
```

### 19.3 Circuit Breaker Implementation

```rust
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};

const CLOSED: u8 = 0;
const OPEN: u8 = 1;
const HALF_OPEN: u8 = 2;

pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU64,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
    threshold: u64,
    timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u64, timeout: Duration) -> Self {
        Self {
            state: AtomicU8::new(CLOSED),
            failure_count: AtomicU64::new(0),
            last_failure_time: Arc::new(Mutex::new(None)),
            threshold,
            timeout,
        }
    }

    pub fn is_open(&self) -> bool {
        let state = self.state.load(Ordering::Relaxed);

        if state == OPEN {
            // Check if timeout has elapsed
            if let Some(last_failure) = *self.last_failure_time.lock().unwrap() {
                if last_failure.elapsed() > self.timeout {
                    // Try half-open
                    self.state.store(HALF_OPEN, Ordering::Relaxed);
                    return false;
                }
            }
            return true;
        }

        false
    }

    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.state.store(CLOSED, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

        if failures >= self.threshold {
            self.trip();
        }
    }

    fn trip(&self) {
        self.state.store(OPEN, Ordering::Relaxed);
        *self.last_failure_time.lock().unwrap() = Some(Instant::now());
        tracing::error!("Circuit breaker opened after {} failures", self.threshold);
    }
}
```

## 20. Conclusion

This technical design document provides a comprehensive blueprint for implementing a production-ready distributed rate limiting service in Rust with gRPC and Redis. The design emphasizes:

- **Distributed Architecture**: Redis/Valkey backend enables horizontal scaling
- **Smooth Rate Limiting**: Leaky bucket algorithm provides consistent traffic flow
- **High Availability**: Sentinel-based failover and fail-open strategy
- **Performance**: Sub-10ms latency with connection pooling and Lua scripts
- **Safety**: Rust's type system and atomic Redis operations prevent races
- **Observability**: Comprehensive logging and metrics for operations
- **Extensibility**: Trait-based design allows future algorithm additions

The Redis-backed leaky bucket implementation provides a robust foundation for protecting downstream services while maintaining high performance and availability.

### Success Criteria

The implementation will be considered successful when it achieves:

- Able to work as above.

### Next Steps

1. Review and approve this design document
2. Set up project structure and dependencies
3. Implement Code in phases (Core Implementation)
4. Have Milestones.
5. Like setup proto and rpc server. then use them and log.
6. Setup redis and use script to make a key.
