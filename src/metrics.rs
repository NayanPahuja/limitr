use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_histogram_vec, register_int_counter_vec,
    register_gauge, CounterVec, HistogramVec, IntCounterVec, Gauge
};

lazy_static! {
    // Request metrics
    pub static ref REQUESTS_TOTAL: CounterVec = register_counter_vec!(
        "rate_limiter_requests_total",
        "Total number of rate limit requests",
        &["domain", "prefix", "allowed"]
    ).unwrap();

    pub static ref REQUESTS_DENIED_TOTAL: CounterVec = register_counter_vec!(
        "rate_limiter_requests_denied_total",
        "Total number of denied requests",
        &["domain", "prefix"]
    ).unwrap();

    pub static ref TOKENS_CONSUMED_TOTAL: CounterVec = register_counter_vec!(
        "rate_limiter_tokens_consumed_total",
        "Total number of tokens consumed",
        &["domain", "prefix"]
    ).unwrap();

    pub static ref DENY_COUNT_TOTAL: IntCounterVec = register_int_counter_vec!(
        "rate_limiter_deny_count_total",
        "Accumulated denied tokens",
        &["domain", "prefix"]
    ).unwrap();

    // Latency metrics
    pub static ref REQUEST_DURATION: HistogramVec = register_histogram_vec!(
        "rate_limiter_request_duration_seconds",
        "Request processing duration in seconds",
        &["domain", "prefix", "allowed"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    ).unwrap();

    pub static ref REDIS_DURATION: HistogramVec = register_histogram_vec!(
        "rate_limiter_redis_duration_seconds",
        "Redis command duration in seconds",
        &["command"],
        vec![0.0001, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5]
    ).unwrap();

    // Redis metrics
    pub static ref REDIS_ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "rate_limiter_redis_errors_total",
        "Total number of Redis errors",
        &["error_type"]
    ).unwrap();

    pub static ref REDIS_SCRIPT_EXECUTIONS_TOTAL: CounterVec = register_counter_vec!(
        "rate_limiter_redis_script_executions_total",
        "Total number of Lua script executions",
        &["result"]
    ).unwrap();

    // Bucket metrics
    pub static ref BUCKET_REMAINING_CAPACITY: HistogramVec = register_histogram_vec!(
        "rate_limiter_bucket_remaining_capacity",
        "Remaining capacity in buckets",
        &["domain", "prefix"],
        vec![0.0, 10.0, 25.0, 50.0, 75.0, 100.0, 250.0, 500.0, 1000.0, 5000.0]
    ).unwrap();

    pub static ref LIMITING_RATE_INDEX: HistogramVec = register_histogram_vec!(
        "rate_limiter_limiting_rate_index",
        "Which rate policy was most restrictive",
        &["domain", "prefix"],
        vec![0.0, 1.0, 2.0, 3.0, 4.0]
    ).unwrap();

    // Config metrics (no labels)
    pub static ref CONFIG_RELOADS_TOTAL: CounterVec = register_counter_vec!(
        "rate_limiter_config_reloads_total",
        "Total number of configuration reloads",
        &["result"]
    ).unwrap();

    pub static ref ACTIVE_DOMAINS: Gauge = register_gauge!(
        "rate_limiter_active_domains_total",
        "Number of active domains configured"
    ).unwrap();
}

/// Record a rate limit request
pub fn record_request(domain: &str, prefix: &str, allowed: bool, duration_secs: f64) {
    let allowed_str = if allowed { "true" } else { "false" };
    REQUESTS_TOTAL
        .with_label_values(&[domain, prefix, allowed_str])
        .inc();
    
    REQUEST_DURATION
        .with_label_values(&[domain, prefix, allowed_str])
        .observe(duration_secs);
}

/// Record denied request
pub fn record_denied(domain: &str, prefix: &str, deny_count: i64) {
    REQUESTS_DENIED_TOTAL
        .with_label_values(&[domain, prefix])
        .inc();
    
    DENY_COUNT_TOTAL
        .with_label_values(&[domain, prefix])
        .inc_by(deny_count as u64);
}

/// Record tokens consumed
pub fn record_tokens_consumed(domain: &str, prefix: &str, cost: i32) {
    TOKENS_CONSUMED_TOTAL
        .with_label_values(&[domain, prefix])
        .inc_by(cost as f64);
}

/// Record bucket remaining capacity
pub fn record_remaining_capacity(domain: &str, prefix: &str, capacity: f64) {
    BUCKET_REMAINING_CAPACITY
        .with_label_values(&[domain, prefix])
        .observe(capacity);
}

/// Record limiting rate policy index
pub fn record_limiting_index(domain: &str, prefix: &str, index: i32) {
    LIMITING_RATE_INDEX
        .with_label_values(&[domain, prefix])
        .observe(index as f64);
}

/// Record Redis operation duration
pub fn record_redis_duration(command: &str, duration_secs: f64) {
    REDIS_DURATION
        .with_label_values(&[command])
        .observe(duration_secs);
}

/// Record Redis error
pub fn record_redis_error(error_type: &str) {
    REDIS_ERRORS_TOTAL
        .with_label_values(&[error_type])
        .inc();
}

/// Record script execution
pub fn record_script_execution(success: bool) {
    let result = if success { "success" } else { "error" };
    REDIS_SCRIPT_EXECUTIONS_TOTAL
        .with_label_values(&[result])
        .inc();
}


/// Update config metrics
pub fn update_config_metrics(domain_count: usize) {
    ACTIVE_DOMAINS.set(domain_count as f64);
}

/// Record config reload
pub fn record_config_reload(success: bool) {
    let result = if success { "success" } else { "error" };
    CONFIG_RELOADS_TOTAL
        .with_label_values(&[result])
        .inc();
}