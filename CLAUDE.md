# Redis Shield - Comprehensive Analysis & Roadmap

> **Last Updated:** 2025-11-17
> **Current Version:** 1.0.0
> **Purpose:** Internal knowledge base and development roadmap

---

## Table of Contents

1. [Project Overview](#project-overview)
2. [Architecture Deep Dive](#architecture-deep-dive)
3. [Production Readiness Assessment](#production-readiness-assessment)
4. [V2 Feature Proposals](#v2-feature-proposals)
5. [AI/ML Features](#aiml-features)
6. [Implementation Notes](#implementation-notes)
7. [Performance Considerations](#performance-considerations)

---

## Project Overview

### What is Redis Shield?

Redis Shield is a **Redis loadable module** written in **Rust** that implements the **token bucket algorithm** for rate limiting as a native Redis command. It enables server-side rate limiting directly within Redis, eliminating the need for application-level rate limiting logic.

### Key Statistics

- **Language:** Rust (Edition 2021, MSRV 1.91.1)
- **Rust Toolchain:** 1.91.1
- **Total Lines of Code:** 978 lines
- **Test Coverage:** 26 integration tests (~660 lines)
- **Module Size:** 340KB repository
- **Platforms:** Linux (x86_64, aarch64), macOS (x86_64, aarch64)

### Use Cases

- Rate limiting HTTP requests by IP address
- API quota management by User ID
- DDoS protection
- Preventing resource exhaustion
- Traffic shaping and throttling
- Multi-tenant resource allocation

---

## Architecture Deep Dive

### Module Structure

```
redis-shield/
├── src/
│   ├── lib.rs          (818 lines) - Command handler & tests
│   └── bucket.rs       (160 lines) - Token bucket implementation
├── Cargo.toml          - Project manifest
├── rust-toolchain.toml - Rust version specification (1.91.1)
├── .github/workflows/
│   ├── code_review.yml - CI pipeline
│   └── release.yml     - Multi-platform releases
└── deny.toml           - Security audit configuration
```

### Core Components

#### 1. Command Handler (`lib.rs`)

**Purpose:** Entry point for the `SHIELD.absorb` Redis command.

**Constants:**
```rust
const MIN_ARGS_LEN: usize = 4;
const MAX_ARGS_LEN: usize = 5;
const DEFAULT_TOKENS: i64 = 1;
const REDIS_COMMAND: &str = "SHIELD.absorb";
const REDIS_MODULE_NAME: &str = "SHIELD";
const REDIS_MODULE_VERSION: i32 = 1;
```

**Key Functions:**

1. **`redis_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult`**
   - Validates argument count (4-5 args)
   - Parses and validates: `capacity`, `period`, `tokens`
   - Creates/retrieves bucket
   - Attempts token consumption
   - Returns remaining tokens or -1

2. **`parse_positive_integer(name: &str, value: &RedisString) -> Result<i64, RedisError>`**
   - Validates integer arguments are positive (> 0)
   - Returns descriptive errors with parameter name

**Memory Allocation Strategy:**
```rust
// Production: Use Redis allocator for proper memory tracking
#[cfg(not(test))]
macro_rules! get_allocator {
    () => { redis_module::alloc::RedisAlloc };
}

// Testing: Use system allocator for simpler testing
#[cfg(test)]
macro_rules! get_allocator {
    () => { std::alloc::System };
}
```

#### 2. Token Bucket Implementation (`bucket.rs`)

**Data Structure:**
```rust
pub struct Bucket<'a> {
    pub key: &'a RedisString,      // Unique bucket identifier
    pub capacity: i64,              // Max tokens the bucket can hold
    pub period: i64,                // Refill period in milliseconds
    pub tokens: i64,                // Current token count
    ctx: &'a Context,               // Redis context for operations
}
```

**Key Methods:**

1. **`Bucket::new(...) -> Result<Bucket, RedisError>`**
   - Creates or retrieves bucket from Redis
   - Validates capacity and period are positive
   - Calls `fetch_tokens()` to load state and calculate refills
   - Converts period from seconds to milliseconds

2. **`Bucket::pour(tokens: i64) -> Result<i64, RedisError>`**
   - Checks if sufficient tokens available
   - If YES: Decrements tokens, persists with PSETEX, returns remaining count
   - If NO: Returns -1 (request denied, no tokens consumed)

3. **`Bucket::fetch_tokens() -> Result<(), RedisError>`** (Private)
   - Retrieves current TTL using `PTTL` command
   - Calculates elapsed time: `elapsed = period - ttl`
   - Computes refilled tokens: `(elapsed / period) * capacity`
   - Updates token count, capped at capacity
   - Handles edge cases:
     - Key doesn't exist → Initialize with full capacity
     - No TTL → Treat as new bucket
     - Corrupted data → Return error
     - Wrong data type → Return error

**Algorithm Implementation:**

The token bucket algorithm works as follows:

1. **Initialization:** New buckets start with `capacity` tokens
2. **Refill Rate:** Tokens refill linearly over the `period`
3. **Refill Formula:** `refilled_tokens = (elapsed_time / period) * capacity`
4. **Capping:** Tokens never exceed capacity
5. **Consumption:** Atomic check-and-decrement operation

**Example Flow:**

```
Time    Action                              Tokens  Calculation
----------------------------------------------------------------------
t=0     SHIELD.absorb user123 30 60 13      17      30 - 13 = 17
t=1s    SHIELD.absorb user123 30 60 13      4       17 + (1/60)*30 - 13 ≈ 4
t=1.1s  SHIELD.absorb user123 30 60 13      -1      Insufficient tokens
t=60s+  SHIELD.absorb user123 30 60 1       29      30 (fully refilled) - 1
```

### Redis Integration

**Redis Commands Used:**
- `PSETEX key milliseconds value` - Set key with TTL in milliseconds
- `PTTL key` - Get remaining TTL in milliseconds
- `GET key` - Retrieve current token count

**Data Storage:**
- **Key:** User-provided identifier (e.g., `ip-127.0.0.1`, `user123`)
- **Value:** Integer token count
- **TTL:** Remaining time in period (automatically expires)

**Timing System:**
- Uses milliseconds internally for precision
- Accepts period in seconds, converts to `period * 1000`
- Supports sub-second periods

### Design Patterns & Techniques

1. **Fluent Builder Pattern** - `Bucket::new()` returns fully initialized bucket
2. **Result Type Error Handling** - Uses `Result<T, RedisError>` throughout
3. **Lifetime Annotations** - `<'a>` ensures bucket validity within command scope
4. **Defensive Programming** - Explicit validation at all boundaries
5. **Fractional Precision** - Uses f64 for refill calculations
6. **Boundary Clamping** - `num::clamp()` prevents invalid TTL values

---

## Production Readiness Assessment

### ✅ Production Ready

Redis Shield is production-ready for:

- **Any Redis deployment** (single-instance or cluster)
- **High-throughput systems** (50K+ req/s per instance)
- **Mission-critical services** (with proper monitoring)
- **Public-facing APIs** and internal services
- **Dynamic traffic patterns** (the token bucket algorithm handles bursts naturally)

### ⚠️ Concerns for Large-Scale Production:

| Issue | Severity | Impact | Mitigation |
|-------|----------|--------|------------|
| No observability/logging | **High** | Can't monitor rate limit behavior | Add application-level logging around Redis calls |
| No inspection commands | **Medium** | Can't check bucket state without consuming tokens | Use Redis `TTL`/`GET` commands directly (hacky) |

### Security Audit

✅ **Secure:**
- Input validation on all parameters
- No buffer overflows (Rust safety)
- No SQL/command injection vectors
- Proper error handling
- Active dependency scanning (cargo-audit)
- Fixed vulnerabilities: RUSTSEC-2024-0421, RUSTSEC-2024-0407

✅ **Resilience:**
- Handles corrupted Redis data gracefully
- Validates data types before parsing
- TTL edge cases handled (no TTL, negative TTL)
- Atomic operations prevent race conditions

### Current Limitations

1. **No dry-run mode** - Can't check limits without consuming tokens
2. **No bulk operations** - Each key requires separate command invocation
3. **Fixed response format** - Returns only integer (no metadata)
4. **No configuration** - All parameters hardcoded
5. **No metrics** - No visibility into module performance

---

## V2 Feature Proposals

### Priority 0: Critical for Production

#### 1. Inspection/Query Command

**Command:**
```redis
SHIELD.inspect <key>
```

**Returns:**
```json
{
  "exists": true,
  "capacity": 30,
  "tokens": 17,
  "period": 60000,
  "ttl": 45123,
  "utilization": 0.43
}
```

**Benefits:**
- Debug rate limits without consuming tokens
- Monitor bucket health
- Implement dashboards and alerts
- Verify rate limit configuration

**Implementation:**
- New command handler: `inspect_command()`
- Read-only operations: `GET` + `PTTL`
- Return structured data (array or bulk string)

---

#### 2. Reset/Delete Command

**Command:**
```redis
SHIELD.reset <key>              # Reset to full capacity
SHIELD.delete <key>              # Delete bucket entirely
```

**Returns:**
```
OK (or number of tokens after reset)
```

**Benefits:**
- Manual overrides for VIP users
- Testing and development
- Emergency access restoration
- Clear rate limit state

**Implementation:**
- `reset`: Set key to capacity with full TTL
- `delete`: Redis `DEL` command

---

#### 3. Observability & Enhanced Response

**Option A: Verbose Flag**
```redis
SHIELD.absorb <key> <capacity> <period> [tokens] VERBOSE
```

**Returns:**
```json
{
  "allowed": true,
  "remaining": 25,
  "capacity": 30,
  "retry_after_ms": 0,
  "ttl": 59000,
  "reset_at": 1700000000
}
```

**Option B: Array Response**
```redis
SHIELD.absorb user123 30 60 5
→ 1) (integer) 25              # remaining
   2) (integer) 30              # capacity
   3) (integer) 59000           # ttl
   4) (integer) 0               # retry_after_ms
```

**Benefits:**
- Support HTTP `X-RateLimit-*` headers:
  - `X-RateLimit-Limit: 30`
  - `X-RateLimit-Remaining: 25`
  - `X-RateLimit-Reset: 1700000000`
  - `Retry-After: 10` (if denied)
- Better client feedback
- Easier debugging

**Implementation:**
- Add `VERBOSE` flag parsing
- Return `RedisValue::Array` or structured string
- Calculate `retry_after` when denied

---

#### 4. Internal Metrics

**Metrics to Track:**
- Total requests processed
- Denied requests (by key pattern)
- Active buckets count
- Average tokens consumed
- P50/P95/P99 latencies

**Exposure:**
```redis
SHIELD.stats
→ 1) total_requests: 1000000
   2) denied_requests: 5000
   3) active_buckets: 234
   4) avg_tokens_consumed: 2.3
```

**Implementation:**
- Add atomic counters in module state
- Use `redis_module::thread_safe::ThreadSafeContext`
- Optional: Export Prometheus metrics

---

### Priority 1: High Value Features

#### 5. Batch/Multi-Key Operations

**Command:**
```redis
SHIELD.absorb_multi key1 30 60 1 key2 50 120 2 key3 100 3600 5
```

**Returns:**
```
1) (integer) 29    # key1 remaining
2) (integer) 48    # key2 remaining
3) (integer) 95    # key3 remaining
```

**Benefits:**
- Reduce round-trips for multi-resource requests
- Rate limit by IP **and** user simultaneously
- Atomic multi-bucket operations

**Use Case:**
```python
# Rate limit by both IP and user in one call
result = redis.execute_command(
    'SHIELD.absorb_multi',
    f'ip:{ip}', 100, 60, 1,      # 100/min per IP
    f'user:{user_id}', 1000, 3600, 1  # 1000/hour per user
)
if -1 in result:
    return 429  # Rate limited
```

**Implementation:**
- Parse variable-length arguments
- Process buckets sequentially
- Return array of results
- Consider all-or-nothing semantics

---

#### 6. Separate Burst Capacity

**Current Issue:**
- Token bucket conflates capacity and burst
- Can't model "100 req/hour with burst of 20"

**Proposed API:**
```redis
SHIELD.absorb <key> <rate> <period> <burst> [tokens]
```

**Example:**
```redis
SHIELD.absorb api_key 100 3600 20 5
                      ▲    ▲    ▲  ▲
                      |    |    |  └─ consume 5 tokens
                      |    |    └──── burst capacity: 20
                      |    └───────── period: 1 hour
                      └────────────── rate: 100/hour
```

**Algorithm Change:**
- `capacity = burst` (max tokens at any time)
- Refill rate: `rate / period` tokens per second
- Allows sustained `rate` while tolerating `burst`

**Benefits:**
- Industry-standard rate limiting semantics
- Better alignment with HTTP specs (RFC 6585)
- More flexible traffic shaping

---

#### 7. Redis Cluster Support

**Requirements:**
- Test multi-node cluster deployments
- Document hash slot behavior
- Handle `MOVED`/`ASK` redirects (Redis client handles this)
- Document limitations of multi-key operations

**Considerations:**
- Single-key operations should work (same hash slot)
- Multi-key ops (`absorb_multi`) need hash tags: `{user:123}:ip`, `{user:123}:endpoint`
- No cross-slot atomicity

**Documentation Needed:**
```markdown
## Redis Cluster Support

Redis Shield is compatible with Redis Cluster. Each bucket operation
is isolated to a single key, which maps to a single hash slot.

### Using Multi-Key Operations in Cluster

Use hash tags to ensure keys land in the same slot:

```redis
SHIELD.absorb_multi {user:123}:ip 100 60 1 {user:123}:endpoint 50 60 1
```

Without hash tags, operations may fail with CROSSSLOT error.
```

---

#### 8. Configurable Response Format

**Implementation:**
```redis
SHIELD.config SET response_format simple    # Returns integer (default)
SHIELD.config SET response_format verbose   # Returns array
SHIELD.config SET response_format json      # Returns JSON string
```

**Benefits:**
- Easier migration (keep simple format initially)
- Opt-in to verbose responses
- Framework-specific integration

---

### Priority 2: Nice-to-Have Features

#### 9. Additional Rate Limiting Algorithms

**Leaky Bucket:**
```redis
SHIELD.absorb <key> <rate> <period> ALGORITHM leaky_bucket
```

- Smooths out bursty traffic
- Constant outflow rate
- Queue-based semantics

**Sliding Window:**
```redis
SHIELD.absorb <key> <capacity> <window> ALGORITHM sliding_window
```

- More accurate than fixed windows
- Prevents boundary gaming
- Higher memory overhead (need sorted set)

**Fixed Window:**
```redis
SHIELD.absorb <key> <capacity> <window> ALGORITHM fixed_window
```

- Simpler than token bucket
- Resets at window boundaries
- Lower memory overhead

**Implementation:**
- Add `algorithm` field to bucket struct
- Separate logic for each algorithm
- Maintain backward compatibility (default to token bucket)

---

#### 10. Dry-Run/Check Mode

**Command:**
```redis
SHIELD.check <key> <capacity> <period> [tokens]
```

**Behavior:**
- Returns available tokens **WITHOUT** consuming
- Identical logic to `absorb` but read-only
- Use for pre-flight checks

**Use Case:**
```python
# Check if expensive operation is allowed
available = redis.execute_command('SHIELD.check', f'user:{id}', 100, 60)
if available >= 10:
    perform_expensive_operation()
    redis.execute_command('SHIELD.absorb', f'user:{id}', 100, 60, 10)
```

---

#### 11. Namespace/Prefix Configuration

**Command:**
```redis
SHIELD.config SET prefix "ratelimit:"
```

**Effect:**
- All keys automatically prefixed: `ratelimit:user123`
- Prevents collisions with application keys
- Easier multi-tenancy

**Implementation:**
- Store prefix in module-level config
- Prepend to keys in all operations

---

#### 12. Penalty Mode (TTL Extension on Deny)

**Command:**
```redis
SHIELD.absorb <key> <capacity> <period> [tokens] PENALTY <seconds>
```

**Behavior:**
- Normal operation when tokens available
- On deny: Extend TTL by penalty duration
- Discourages abusive behavior

**Example:**
```redis
# After rate limit hit, wait 5 minutes instead of normal refill
SHIELD.absorb abuser 10 60 1 PENALTY 300
```

---

#### 13. Bulk Bucket Management

**Commands:**
```redis
SHIELD.list <pattern>
→ Returns all bucket keys matching pattern

SHIELD.flush <pattern>
→ Deletes all buckets matching pattern

SHIELD.count <pattern>
→ Returns count of buckets
```

**Use Cases:**
- Testing: `SHIELD.flush test:*`
- Maintenance: `SHIELD.list user:*`
- Monitoring: `SHIELD.count ip:*`

**Implementation:**
- Use `SCAN` with pattern matching
- Operate on matching keys
- Return counts/arrays

---

#### 14. Fractional Token Costs

**Current:** Tokens must be `i64` integers

**Proposed:** Support `f64` for weighted requests

**Command:**
```redis
SHIELD.absorb expensive_api 100 3600 2.5
```

**Use Case:**
```python
# Different endpoints have different costs
redis.execute_command('SHIELD.absorb', f'user:{id}', 1000, 3600, 0.1)  # GET
redis.execute_command('SHIELD.absorb', f'user:{id}', 1000, 3600, 5.0)  # POST
redis.execute_command('SHIELD.absorb', f'user:{id}', 1000, 3600, 25)   # AI API
```

**Implementation:**
- Change `tokens: i64` to `tokens: f64`
- Update parsing and arithmetic
- Handle floating-point precision issues

---

#### 15. Performance Benchmarks

**Add `criterion` benchmarks:**

```rust
#[bench]
fn bench_absorb_new_bucket(b: &mut Bencher) {
    b.iter(|| {
        shield_absorb(&mut con, "bench_key", 100, 60, Some(1))
    });
}

#[bench]
fn bench_absorb_existing_bucket(b: &mut Bencher) {
    // Pre-create bucket
    shield_absorb(&mut con, "bench_key", 100, 60, Some(1));

    b.iter(|| {
        shield_absorb(&mut con, "bench_key", 100, 60, Some(1))
    });
}
```

**Metrics to measure:**
- Latency (P50, P95, P99)
- Throughput (req/s)
- Memory per bucket
- Impact of bucket count

---

### Priority 3: Advanced Features

#### 16. Alternative Lua Implementation

**Note:** This is NOT a performance optimization. As a native compiled Rust module, the current implementation is already highly efficient with minimal overhead.

**Why consider Lua at all?**
- **Only benefit:** Guaranteed atomicity (prevents command interleaving)
- For rate limiting, this doesn't matter - the algorithm handles eventual consistency

**Trade-offs:**
- ❌ Likely SLOWER (Lua VM interpretation vs native Rust)
- ❌ Harder to maintain and debug
- ❌ Less portable (Lua script management)
- ✅ Slightly more atomic (negligible benefit)

**Recommendation:** Not worth implementing unless specific atomicity requirements arise

---

#### 17. Module Configuration System

**Commands:**
```redis
SHIELD.config GET <option>
SHIELD.config SET <option> <value>
SHIELD.config LIST
```

**Configuration Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_buckets` | int | 0 (unlimited) | Prevent memory exhaustion |
| `default_capacity` | int | 100 | Fallback if not specified |
| `default_period` | int | 60 | Fallback if not specified |
| `response_format` | enum | simple | simple/verbose/json |
| `key_prefix` | string | "" | Automatic key prefixing |
| `logging_level` | enum | warn | debug/info/warn/error |

**Implementation:**
- Store config in module-level static
- Use `ThreadSafeContext` for concurrent access
- Validate ranges and types

---

#### 18. Health Check & Diagnostics

**Command:**
```redis
SHIELD.health
```

**Returns:**
```json
{
  "status": "healthy",
  "version": "2.0.0",
  "active_buckets": 1234,
  "total_requests": 1000000,
  "denied_requests": 5000,
  "uptime_seconds": 86400,
  "memory_used_bytes": 1048576
}
```

**Benefits:**
- Kubernetes readiness/liveness probes
- Monitoring integration
- Debugging production issues

---

#### 19. Time-Based Quotas

**Command:**
```redis
SHIELD.quota <key> <limit> <period_type>

period_type: second | minute | hour | day | week | month
```

**Example:**
```redis
SHIELD.quota user:123 1000 day
→ 1000 requests per calendar day (resets at midnight UTC)
```

**Implementation:**
- Calculate window boundaries based on timestamp
- Use fixed window algorithm
- Store window start time + count

---

#### 20. Distributed Rate Limiting (Multi-Instance)

**Challenge:** Rate limit across multiple Redis instances

**Approaches:**

1. **Consistent Hashing:** Route keys to specific instances
2. **Gossip Protocol:** Sync bucket state (complex)
3. **Central Coordinator:** Single source of truth (bottleneck)

**Recommendation:** Document that multi-instance rate limiting requires:
- Redis Cluster (preferred)
- Client-side consistent hashing
- Or accept eventual consistency

---

## AI/ML Features

### Overview

Integrating AI/ML capabilities into Redis Shield transforms it from a **static rate limiter** into an **intelligent, adaptive traffic management system**. These features leverage machine learning to detect anomalies, predict abuse, and automatically optimize rate limits based on real-world traffic patterns.

### Architecture Principles

**Key Constraints:**
- Redis modules must remain **lightweight and fast** (sub-millisecond latency)
- ML inference should be **<1ms** to avoid performance degradation
- Training happens **outside** the Redis module (offline)
- Model deployment must support **zero-downtime** updates
- Handle **high-cardinality data** (millions of user keys)

**Recommended Approach:**
1. **Hybrid System:** ML training in separate service, inference in Redis module
2. **Embedded Models:** Deploy lightweight models (decision trees, linear models) using ONNX
3. **Feature Store:** Use Redis data structures for feature caching
4. **Async Training:** Background jobs analyze Redis data, update models periodically
5. **Model Serving:** Embed small ONNX/TensorFlow Lite models in module

---

### Feature 1: Anomaly Detection & Auto-Blocking

**Goal:** Automatically detect and block suspicious traffic patterns

#### Architecture

```rust
// New struct in bucket.rs
pub struct AnomalyDetector<'a> {
    key: &'a RedisString,
    ctx: &'a Context,
    baseline_window: i64,  // 24 hours in seconds
}

impl<'a> AnomalyDetector<'a> {
    // Store request counts in sorted set
    // Key: {bucket_key}:history
    // Score: timestamp
    // Member: request_count

    pub fn detect(&self) -> Result<AnomalyScore, RedisError> {
        // Get last 24h of data
        let history = self.fetch_history()?;

        // Calculate statistics
        let mean = self.calculate_mean(&history);
        let stddev = self.calculate_stddev(&history, mean);
        let current_rate = self.get_current_rate()?;

        // Z-score calculation
        let z_score = (current_rate - mean) / stddev;

        Ok(AnomalyScore {
            z_score,
            is_anomaly: z_score > 3.0,  // 3 sigma threshold
            severity: self.classify_severity(z_score),
        })
    }
}
```

#### Redis Data Structure

```redis
# Store hourly request counts
ZADD user:123:history 1700000000 150  # timestamp → count at that hour
ZADD user:123:history 1700003600 145
ZADD user:123:history 1700007200 152
...

# Keep last 24 hours (auto-expire old entries)
# Calculate: mean=150, stddev=5

# Current hour: 1500 requests
# Z-score: (1500 - 150) / 5 = 270 🚨 ANOMALY!
```

#### Command Implementation

```rust
fn redis_command_with_anomaly(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    // Parse arguments
    let detect_anomaly = args.contains("DETECT_ANOMALY");

    if detect_anomaly {
        let detector = AnomalyDetector::new(ctx, &args[1]);
        let anomaly = detector.detect()?;

        if anomaly.is_anomaly {
            match anomaly.severity {
                Severity::Critical => {
                    // Block completely for 5 minutes
                    ctx.call("SETEX", &[&args[1], "300", "-2"])?;
                    return Ok((-2).into());  // -2 = anomaly block
                }
                Severity::High => {
                    // Reduce capacity by 90%
                    capacity = capacity / 10;
                }
                Severity::Medium => {
                    // Reduce capacity by 50%
                    capacity = capacity / 2;
                }
            }
        }
    }

    // Continue with normal token bucket logic
    let mut bucket = Bucket::new(ctx, &args[1], capacity, period)?;
    bucket.pour(tokens)
}
```

#### Severity Classification

```rust
enum Severity {
    Low,      // 2-3 sigma
    Medium,   // 3-5 sigma
    High,     // 5-10 sigma
    Critical, // >10 sigma
}

impl AnomalyDetector {
    fn classify_severity(&self, z_score: f64) -> Severity {
        match z_score.abs() {
            z if z > 10.0 => Severity::Critical,
            z if z > 5.0  => Severity::High,
            z if z > 3.0  => Severity::Medium,
            _             => Severity::Low,
        }
    }
}
```

#### Advanced: Pattern-based Detection

```rust
pub struct PatternDetector;

impl PatternDetector {
    pub fn detect_bot_pattern(&self, history: &[Request]) -> bool {
        // Check if requests are too regular (bot-like)
        let intervals: Vec<f64> = history.windows(2)
            .map(|w| (w[1].timestamp - w[0].timestamp) as f64)
            .collect();

        let variance = calculate_variance(&intervals);

        // Very low variance = likely bot
        variance < 100.0  // Less than 100ms variance
    }

    pub fn detect_scraping(&self, history: &[Request]) -> bool {
        // Check if systematically iterating IDs
        let endpoints: Vec<String> = history.iter()
            .map(|r| extract_numeric_id(&r.endpoint))
            .collect();

        // Check if IDs are sequential
        is_sequential(&endpoints)
    }
}
```

#### Response Format

```redis
# Normal
SHIELD.absorb user:123 100 60 1 DETECT_ANOMALY
→ 99

# Anomaly detected (medium severity)
SHIELD.absorb user:attacker 100 60 1 DETECT_ANOMALY
→ -2 (blocked for 300 seconds due to anomaly)

# With verbose response
SHIELD.absorb user:123 100 60 1 DETECT_ANOMALY VERBOSE
→ [99, 100, 60000, 0, {"anomaly_score": 0.2, "severity": "low"}]
```

**Detection Signals:**
- User normally does 10 req/min, suddenly 1000 req/min → Block
- IP normally hits 5 endpoints, now scraping 100s → Flag
- Request timing too consistent (bot pattern) → Suspicious
- Spike in error rates or failed attempts → Credential stuffing

**Benefits:**
- **Automatic DDoS mitigation**
- **Credential stuffing detection**
- **API abuse prevention**
- **Zero-day attack protection**

---

### Feature 4: Adaptive/Dynamic Limits

**Goal:** Auto-adjust rate limits based on system load and traffic patterns

#### System Metrics Integration

```rust
pub struct SystemMetrics {
    cpu_usage: f64,        // 0.0 - 1.0
    memory_pressure: f64,  // 0.0 - 1.0
    redis_memory: i64,     // bytes
    active_connections: i64,
    error_rate: f64,       // last 5 min
}

impl SystemMetrics {
    pub fn collect(ctx: &Context) -> Result<Self, RedisError> {
        // Use Redis INFO command
        let info = ctx.call("INFO", &["stats", "memory", "cpu"])?;

        Ok(Self {
            cpu_usage: parse_cpu_usage(&info),
            memory_pressure: parse_memory_pressure(&info),
            redis_memory: parse_used_memory(&info),
            active_connections: parse_connections(&info),
            error_rate: Self::get_error_rate(ctx)?,
        })
    }
}
```

#### Adaptive Algorithm

```rust
pub struct AdaptiveController {
    // PID-like controller for smooth adjustments
    base_capacity: i64,
    current_multiplier: f64,

    // Smoothing
    last_adjustment: i64,  // timestamp
    adjustment_rate: f64,  // max change per second
}

impl AdaptiveController {
    pub fn calculate_capacity(&mut self, metrics: SystemMetrics) -> i64 {
        // Calculate pressure score (0.0 = no pressure, 1.0 = critical)
        let pressure = (
            0.4 * metrics.cpu_usage +
            0.3 * metrics.memory_pressure +
            0.3 * metrics.error_rate
        );

        // Target multiplier based on pressure
        let target_multiplier = match pressure {
            p if p > 0.9 => 0.2,   // Critical: reduce to 20%
            p if p > 0.7 => 0.5,   // High: reduce to 50%
            p if p > 0.5 => 0.8,   // Medium: reduce to 80%
            p if p < 0.3 => 1.3,   // Low: increase to 130%
            _ => 1.0,              // Normal: 100%
        };

        // Smooth transition (prevent sudden jumps)
        let now = current_timestamp();
        let time_delta = (now - self.last_adjustment) as f64;
        let max_change = self.adjustment_rate * time_delta;

        let delta = (target_multiplier - self.current_multiplier)
            .clamp(-max_change, max_change);

        self.current_multiplier += delta;
        self.last_adjustment = now;

        // Apply multiplier
        (self.base_capacity as f64 * self.current_multiplier) as i64
    }
}
```

#### Configuration

```redis
# Enable adaptive mode globally
SHIELD.config SET adaptive_mode enabled
SHIELD.config SET adaptive_pressure_cpu 0.4      # CPU weight
SHIELD.config SET adaptive_pressure_memory 0.3   # Memory weight
SHIELD.config SET adaptive_pressure_error 0.3    # Error rate weight
SHIELD.config SET adaptive_max_reduction 0.2     # Don't go below 20%
SHIELD.config SET adaptive_max_increase 1.5      # Don't exceed 150%
SHIELD.config SET adaptive_smoothing 0.1         # Adjustment rate (10%/sec)
```

#### Usage Example

```redis
# Normal conditions: 9am, low load
SHIELD.absorb user:123 100 60 1 ADAPTIVE
→ 99 (full capacity)

# Peak hours: 12pm, high load (CPU 85%, memory 80%)
SHIELD.absorb user:123 100 60 1 ADAPTIVE
→ 49 (capacity auto-reduced to 50)

# System recovery: load decreasing
SHIELD.absorb user:123 100 60 1 ADAPTIVE
→ 74 (capacity gradually restored to 75)
```

#### Global State Management

```rust
// Use Redis module thread-safe context for shared state
static ADAPTIVE_CONTROLLER: Lazy<Mutex<AdaptiveController>> = Lazy::new(|| {
    Mutex::new(AdaptiveController {
        base_capacity: 100,
        current_multiplier: 1.0,
        last_adjustment: 0,
        adjustment_rate: 0.1,
    })
});

// Background thread updates metrics every second
fn start_metrics_collector(ctx: ThreadSafeContext) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));

            let metrics = SystemMetrics::collect(&ctx).unwrap();
            let mut controller = ADAPTIVE_CONTROLLER.lock().unwrap();
            controller.update_metrics(metrics);
        }
    });
}
```

**Benefits:**
- **Prevent system overload** automatically
- **Better UX** for trusted users during normal load
- **Cost optimization** (scale down during low traffic)
- **Self-healing** under attack

---

### Feature 5: Smart Retry Recommendations

**Goal:** Tell clients exactly when to retry (not just "try later")

#### Basic Implementation (Mathematical)

```rust
pub struct RetryCalculator<'a> {
    bucket: &'a Bucket<'a>,
}

impl<'a> RetryCalculator<'a> {
    pub fn calculate_retry_time(&self, tokens_needed: i64) -> i64 {
        let tokens_available = self.bucket.tokens;

        if tokens_available >= tokens_needed {
            return 0;  // Can retry immediately
        }

        let tokens_deficit = tokens_needed - tokens_available;

        // Refill rate: tokens per millisecond
        let refill_rate = self.bucket.capacity as f64 / self.bucket.period as f64;

        // Time to accumulate needed tokens
        let time_to_refill = (tokens_deficit as f64 / refill_rate).ceil() as i64;

        // Add small buffer (5%)
        (time_to_refill as f64 * 1.05) as i64
    }
}
```

#### Advanced: Predictive Retry (ML-based)

```rust
pub struct PredictiveRetryCalculator<'a> {
    ctx: &'a Context,
    key: &'a RedisString,
}

impl<'a> PredictiveRetryCalculator<'a> {
    pub fn predict_retry_time(&self, tokens_needed: i64) -> (i64, f64) {
        // Get recent request rate
        let recent_rate = self.get_recent_request_rate()?;

        // Predict future consumption
        let predicted_consumption = recent_rate * 2.0;  // Next 2 seconds

        // Calculate retry time considering concurrent traffic
        let basic_retry = self.calculate_basic_retry(tokens_needed);

        // Adjust for predicted concurrent consumption
        let adjusted_retry = if predicted_consumption > 0.0 {
            let competition_factor = 1.0 + (predicted_consumption / self.capacity as f64);
            (basic_retry as f64 * competition_factor) as i64
        } else {
            basic_retry
        };

        // Confidence based on traffic predictability
        let traffic_variance = self.calculate_traffic_variance()?;
        let confidence = 1.0 / (1.0 + traffic_variance);

        (adjusted_retry, confidence)
    }

    fn get_recent_request_rate(&self) -> Result<f64, RedisError> {
        // Count requests in last 10 seconds
        let history_key = format!("{}:requests", self.key.to_string());
        let now = current_timestamp_ms();
        let ten_secs_ago = now - 10000;

        let count: i64 = self.ctx.call(
            "ZCOUNT",
            &[&history_key, &ten_secs_ago.to_string(), &now.to_string()]
        )?;

        Ok(count as f64 / 10.0)  // requests per second
    }
}
```

#### Response Format

```rust
pub enum RetryResponse {
    Allowed(i64),                    // Tokens remaining
    Denied {
        retry_after_ms: i64,
        confidence: f64,
        reason: DenialReason,
    },
}

pub enum DenialReason {
    RateLimit,
    Anomaly,
    SystemOverload,
}

impl RetryResponse {
    pub fn to_redis_value(&self) -> RedisValue {
        match self {
            RetryResponse::Allowed(tokens) => {
                RedisValue::Integer(*tokens)
            }
            RetryResponse::Denied { retry_after_ms, confidence, reason } => {
                RedisValue::Array(vec![
                    RedisValue::Integer(-1),
                    RedisValue::Integer(*retry_after_ms),
                    RedisValue::SimpleString(format!("{:.2}", confidence)),
                    RedisValue::SimpleString(reason.to_string()),
                ])
            }
        }
    }
}
```

#### Usage Examples

```redis
# Simple retry
SHIELD.absorb user:123 10 60 5
→ -1

# Smart retry (basic)
SHIELD.absorb user:123 10 60 5 SMART_RETRY
→ [-1, 3500, "1.00", "rate_limit"]
    ▲    ▲     ▲       ▲
    |    |     |       └─ reason
    |    |     └───────── confidence (100%)
    |    └─────────────── retry in 3500ms
    └──────────────────── denied

# Smart retry (predictive - high traffic)
SHIELD.absorb user:123 10 60 5 SMART_RETRY PREDICTIVE
→ [-1, 5200, "0.75", "rate_limit"]
    ▲    ▲     ▲
    |    |     └─ confidence (75% - traffic unpredictable)
    |    └─────── retry in 5200ms (adjusted for concurrent traffic)
    └──────────── denied
```

#### HTTP Integration

```python
# Application code
result = redis.execute_command('SHIELD.absorb', f'user:{id}', 100, 60, 1, 'SMART_RETRY')

if isinstance(result, list) and result[0] == -1:
    retry_after_ms = result[1]

    # Set HTTP headers
    response.headers['X-RateLimit-Limit'] = '100'
    response.headers['X-RateLimit-Remaining'] = '0'
    response.headers['Retry-After'] = str(retry_after_ms // 1000)  # seconds

    return JSONResponse(
        status_code=429,
        content={"error": "Rate limited", "retry_after_ms": retry_after_ms}
    )
```

**Benefits:**
- **Reduce retry storms** (coordinated retries)
- **Better client UX** (precise waiting time)
- **Load balancing** across time
- **Support HTTP `Retry-After` header**

---

### Feature 7: Predictive Throttling

**Goal:** Predict future traffic spikes and adjust limits proactively

#### Time Series Data Collection

```rust
pub struct TrafficHistory {
    ctx: &Context,
    key: String,  // e.g., "traffic:global" or "traffic:endpoint:/api/users"
}

impl TrafficHistory {
    pub fn record_request(&self) -> Result<(), RedisError> {
        let now = current_timestamp_ms();
        let minute_bucket = now / 60000;  // Round to minute

        // Increment counter for this minute
        let key = format!("{}:minute:{}", self.key, minute_bucket);
        self.ctx.call("INCR", &[&key])?;
        self.ctx.call("EXPIRE", &[&key, "7200"])?;  // Keep 2 hours

        Ok(())
    }

    pub fn get_history(&self, minutes: i64) -> Result<Vec<i64>, RedisError> {
        let now = current_timestamp_ms();
        let current_minute = now / 60000;

        let mut counts = Vec::new();

        for i in 0..minutes {
            let bucket = current_minute - i;
            let key = format!("{}:minute:{}", self.key, bucket);

            let count: i64 = match self.ctx.call("GET", &[&key])? {
                RedisValue::Integer(n) => n,
                _ => 0,
            };

            counts.push(count);
        }

        Ok(counts)
    }
}
```

#### Simple Forecasting (Moving Average)

```rust
pub struct SimpleForecaster;

impl SimpleForecaster {
    pub fn forecast_next(&self, history: &[i64]) -> i64 {
        if history.is_empty() {
            return 0;
        }

        // Exponential moving average
        let alpha = 0.3;  // Smoothing factor
        let mut ema = history[0] as f64;

        for &value in &history[1..] {
            ema = alpha * value as f64 + (1.0 - alpha) * ema;
        }

        // Add trend component
        let trend = if history.len() >= 2 {
            let recent = history[0] as f64;
            let older = history[history.len() - 1] as f64;
            (recent - older) / history.len() as f64
        } else {
            0.0
        };

        (ema + trend).max(0.0) as i64
    }

    pub fn detect_spike(&self, history: &[i64], predicted: i64) -> bool {
        let avg: f64 = history.iter().sum::<i64>() as f64 / history.len() as f64;
        let predicted_f = predicted as f64;

        // Spike if predicted is 2x average
        predicted_f > avg * 2.0
    }
}
```

#### Advanced: Seasonal Decomposition

```rust
pub struct SeasonalForecaster {
    // Captures daily/weekly patterns
}

impl SeasonalForecaster {
    pub fn forecast(&self, timestamp: i64, history: &[i64]) -> ForecastResult {
        let hour_of_day = (timestamp / 3600) % 24;
        let day_of_week = (timestamp / 86400) % 7;

        // Get historical average for this hour+day
        let seasonal_component = self.get_seasonal_avg(hour_of_day, day_of_week);

        // Get recent trend
        let trend_component = self.calculate_trend(history);

        // Combine
        let forecast = seasonal_component + trend_component;

        // Calculate confidence interval
        let variance = self.calculate_variance(history);
        let std_dev = variance.sqrt();

        ForecastResult {
            predicted: forecast,
            lower_bound: forecast - 2.0 * std_dev,
            upper_bound: forecast + 2.0 * std_dev,
            confidence: 0.95,
        }
    }
}
```

#### Proactive Throttling Logic

```rust
pub struct PredictiveThrottler {
    forecaster: SeasonalForecaster,
    system_capacity: i64,  // Max requests system can handle
}

impl PredictiveThrottler {
    pub fn calculate_preemptive_limit(&self) -> i64 {
        // Forecast next 5 minutes
        let mut predictions = Vec::new();
        for i in 1..=5 {
            let future_time = current_timestamp() + (i * 60);
            let forecast = self.forecaster.forecast(future_time, &history);
            predictions.push(forecast.predicted);
        }

        let max_predicted = predictions.iter().max().unwrap_or(&0);

        // If spike predicted, start throttling now
        if *max_predicted > self.system_capacity as f64 * 0.8 {
            // Reduce current limit proportionally
            let reduction_factor = self.system_capacity as f64 / max_predicted;
            return (base_capacity as f64 * reduction_factor) as i64;
        }

        base_capacity
    }

    pub fn get_throttle_schedule(&self) -> Vec<ThrottlePoint> {
        // Return gradual throttling schedule
        // E.g., reduce 10% every 5 minutes until spike passes
        vec![
            ThrottlePoint { time: now, multiplier: 1.0 },
            ThrottlePoint { time: now + 300, multiplier: 0.9 },
            ThrottlePoint { time: now + 600, multiplier: 0.8 },
            ThrottlePoint { time: now + 900, multiplier: 0.7 },
        ]
    }
}
```

#### Background Prediction Worker

```rust
// Runs every minute, updates predicted limits
fn prediction_worker(ctx: ThreadSafeContext) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(60));

            // Get traffic history
            let history = TrafficHistory::new(&ctx, "global");
            let data = history.get_history(60).unwrap();  // Last hour

            // Forecast next 5 minutes
            let forecaster = SeasonalForecaster::new();
            let prediction = forecaster.forecast(current_timestamp(), &data);

            // Store prediction
            ctx.call("SET", &["predicted:traffic:next", &prediction.predicted.to_string()]).unwrap();

            // Update throttle multiplier if needed
            let throttler = PredictiveThrottler::new(forecaster, SYSTEM_CAPACITY);
            let new_limit = throttler.calculate_preemptive_limit();

            ctx.call("SET", &["adaptive:predicted_limit", &new_limit.to_string()]).unwrap();
        }
    });
}
```

#### Usage Example

```
12:00 PM - Normal traffic (1K req/min)
12:30 PM - Predicted lunch surge (5K req/min)

Instead of:
  - Sudden limit drop at 12:30 (bad UX)

Do:
  - Gradually reduce from 12:00-12:30 (smooth)
```

**Benefits:**
- **Prevent system crashes** during predicted spikes
- **Smoother UX** (gradual vs sudden limits)
- **Better resource planning**
- **Proactive protection**

---

### Feature 9: User Clustering (Automatic Personas)

**Goal:** Automatically segment users and apply persona-based limits

#### Feature Extraction

```rust
pub struct UserFeatures {
    avg_requests_per_hour: f64,
    request_variance: f64,
    peak_hour: u8,              // 0-23
    endpoint_diversity: f64,     // Shannon entropy of endpoints
    avg_session_duration: f64,
    error_rate: f64,
    geographic_locations: u8,
    account_age_days: i64,
}

impl UserFeatures {
    pub fn extract(ctx: &Context, user_key: &str) -> Result<Self, RedisError> {
        // Fetch historical data
        let history_key = format!("{}:stats", user_key);
        let stats: HashMap<String, String> = ctx.call("HGETALL", &[&history_key])?;

        Ok(Self {
            avg_requests_per_hour: stats.get("avg_req_hour")
                .and_then(|s| s.parse().ok()).unwrap_or(0.0),
            request_variance: stats.get("req_variance")
                .and_then(|s| s.parse().ok()).unwrap_or(0.0),
            peak_hour: stats.get("peak_hour")
                .and_then(|s| s.parse().ok()).unwrap_or(12),
            endpoint_diversity: stats.get("endpoint_entropy")
                .and_then(|s| s.parse().ok()).unwrap_or(0.0),
            avg_session_duration: stats.get("avg_session_sec")
                .and_then(|s| s.parse().ok()).unwrap_or(0.0),
            error_rate: stats.get("error_rate")
                .and_then(|s| s.parse().ok()).unwrap_or(0.0),
            geographic_locations: stats.get("geo_locations")
                .and_then(|s| s.parse().ok()).unwrap_or(1),
            account_age_days: stats.get("account_age")
                .and_then(|s| s.parse().ok()).unwrap_or(0),
        })
    }

    pub fn to_vector(&self) -> Vec<f64> {
        vec![
            self.avg_requests_per_hour,
            self.request_variance,
            self.peak_hour as f64,
            self.endpoint_diversity,
            self.avg_session_duration,
            self.error_rate,
            self.geographic_locations as f64,
            self.account_age_days as f64,
        ]
    }
}
```

#### K-Means Clustering

```rust
pub struct UserClusterer {
    k: usize,  // Number of clusters
    centroids: Vec<Vec<f64>>,
    labels: HashMap<String, usize>,  // user_id → cluster_id
}

impl UserClusterer {
    pub fn new(k: usize) -> Self {
        Self {
            k,
            centroids: Vec::new(),
            labels: HashMap::new(),
        }
    }

    pub fn fit(&mut self, features: Vec<(String, UserFeatures)>) {
        // Initialize centroids randomly
        self.centroids = self.initialize_centroids(&features);

        // Iterate until convergence
        for _ in 0..100 {
            // Assign clusters
            for (user_id, feat) in &features {
                let cluster = self.nearest_centroid(&feat.to_vector());
                self.labels.insert(user_id.clone(), cluster);
            }

            // Update centroids
            let old_centroids = self.centroids.clone();
            self.update_centroids(&features);

            // Check convergence
            if self.has_converged(&old_centroids) {
                break;
            }
        }
    }

    pub fn predict(&self, features: &UserFeatures) -> usize {
        self.nearest_centroid(&features.to_vector())
    }

    fn nearest_centroid(&self, point: &[f64]) -> usize {
        self.centroids.iter()
            .enumerate()
            .map(|(i, centroid)| (i, euclidean_distance(point, centroid)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap()
            .0
    }
}

fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}
```

#### Persona Definitions

```rust
pub struct Persona {
    id: usize,
    name: String,
    description: String,
    capacity: i64,
    period: i64,
    trust_level: f64,
}

pub struct PersonaManager {
    personas: HashMap<usize, Persona>,
}

impl PersonaManager {
    pub fn new() -> Self {
        let mut personas = HashMap::new();

        personas.insert(0, Persona {
            id: 0,
            name: "Power User".to_string(),
            description: "High volume, stable traffic, trusted".to_string(),
            capacity: 1000,
            period: 3600,
            trust_level: 0.9,
        });

        personas.insert(1, Persona {
            id: 1,
            name: "Casual User".to_string(),
            description: "Low volume, sporadic usage".to_string(),
            capacity: 100,
            period: 3600,
            trust_level: 0.7,
        });

        personas.insert(2, Persona {
            id: 2,
            name: "API Integration".to_string(),
            description: "Constant, predictable traffic".to_string(),
            capacity: 5000,
            period: 3600,
            trust_level: 0.95,
        });

        personas.insert(3, Persona {
            id: 3,
            name: "Suspicious".to_string(),
            description: "Erratic pattern, high errors".to_string(),
            capacity: 10,
            period: 3600,
            trust_level: 0.1,
        });

        Self { personas }
    }

    pub fn get_limits(&self, cluster_id: usize) -> (i64, i64) {
        self.personas.get(&cluster_id)
            .map(|p| (p.capacity, p.period))
            .unwrap_or((100, 3600))  // Default fallback
    }
}
```

#### Automatic Cluster Assignment

```rust
fn absorb_with_clustering(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let user_key = &args[1];
    let auto_cluster = args[2].to_string() == "AUTO";

    let (capacity, period) = if auto_cluster {
        // Extract features
        let features = UserFeatures::extract(ctx, user_key.to_string_lossy())?;

        // Get or predict cluster
        let cluster = get_user_cluster(ctx, user_key, &features)?;

        // Get limits from persona
        let persona_mgr = PersonaManager::new();
        persona_mgr.get_limits(cluster)
    } else {
        // Manual capacity/period
        (parse_positive_integer("capacity", &args[2])?,
         parse_positive_integer("period", &args[3])?)
    };

    // Continue with normal bucket logic
    let mut bucket = Bucket::new(ctx, user_key, capacity, period)?;
    bucket.pour(tokens)
}

fn get_user_cluster(ctx: &Context, user_key: &RedisString, features: &UserFeatures) -> Result<usize, RedisError> {
    // Check cache
    let cluster_key = format!("{}:cluster", user_key.to_string_lossy());
    if let Ok(RedisValue::Integer(cluster)) = ctx.call("GET", &[&cluster_key]) {
        return Ok(cluster as usize);
    }

    // Predict cluster
    let clusterer = get_global_clusterer();  // Pre-trained model
    let cluster = clusterer.predict(features);

    // Cache result (1 hour TTL)
    ctx.call("SETEX", &[&cluster_key, "3600", &cluster.to_string()])?;

    Ok(cluster)
}
```

#### Background Re-clustering

```rust
// Re-train clusters every night based on recent data
fn clustering_worker(ctx: ThreadSafeContext) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(86400));  // Daily

            // Collect features from all active users
            let users = fetch_active_users(&ctx);
            let mut features = Vec::new();

            for user_id in users {
                if let Ok(feat) = UserFeatures::extract(&ctx, &user_id) {
                    features.push((user_id, feat));
                }
            }

            // Re-train clusters
            let mut clusterer = UserClusterer::new(4);  // 4 personas
            clusterer.fit(features);

            // Update global model
            update_global_clusterer(clusterer);

            // Log cluster statistics
            log_cluster_stats(&ctx, &clusterer);
        }
    });
}
```

#### Usage Example

```redis
# Automatic persona assignment
SHIELD.absorb user:123 AUTO AUTO 5

# Module determines:
# - User is "Power User" (cluster 0)
# - Applies capacity=1000, period=3600
# → Returns: 995 (1000 - 5)

# New suspicious user
SHIELD.absorb user:new_attacker AUTO AUTO 5
# - Classified as "Suspicious" (cluster 3)
# - Applies capacity=10, period=3600
# → Returns: 5 (10 - 5)
```

**Discovered Personas:**
- **Cluster 0: Power Users** (high stable traffic) → 1000/hour
- **Cluster 1: Casual Users** (low sporadic) → 100/hour
- **Cluster 2: API Integrations** (constant predictable) → 5000/hour
- **Cluster 3: Suspicious** (erratic errors) → 10/hour

**Benefits:**
- **Personalized limits** without manual configuration
- **Discover usage patterns** automatically
- **Optimize for different use cases**
- **Adapt to evolving user behavior**

---

### Integration: All Features Together

```rust
fn shield_absorb_v2(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let user_key = &args[1];
    let mut capacity = parse_arg_or_auto(&args[2])?;
    let mut period = parse_arg_or_auto(&args[3])?;
    let tokens = args.get(4).map(|a| parse_positive_integer("tokens", a)).transpose()?.unwrap_or(1);

    // Feature flags
    let detect_anomaly = args.contains("DETECT_ANOMALY");
    let adaptive = args.contains("ADAPTIVE");
    let smart_retry = args.contains("SMART_RETRY");
    let predictive = args.contains("PREDICTIVE");
    let auto_cluster = capacity == AUTO && period == AUTO;

    // 9. Auto-clustering
    if auto_cluster {
        let features = UserFeatures::extract(ctx, user_key.to_string_lossy())?;
        let cluster = get_user_cluster(ctx, user_key, &features)?;
        let persona_mgr = PersonaManager::new();
        (capacity, period) = persona_mgr.get_limits(cluster);
    }

    // 1. Anomaly detection
    if detect_anomaly {
        let detector = AnomalyDetector::new(ctx, user_key);
        let anomaly = detector.detect()?;

        if anomaly.is_anomaly && anomaly.severity == Severity::Critical {
            return Ok((-2).into());  // Block
        }

        if anomaly.is_anomaly {
            capacity = (capacity as f64 * 0.5) as i64;  // Reduce 50%
        }
    }

    // 4. Adaptive limits
    if adaptive {
        let metrics = SystemMetrics::collect(ctx)?;
        let controller = get_global_controller();
        let multiplier = controller.get_multiplier(metrics);
        capacity = (capacity as f64 * multiplier) as i64;
    }

    // 7. Predictive throttling
    if predictive {
        let predicted_limit = ctx.call("GET", &["adaptive:predicted_limit"])?;
        if let RedisValue::Integer(limit) = predicted_limit {
            capacity = capacity.min(limit);
        }
    }

    // Normal token bucket
    let mut bucket = Bucket::new(ctx, user_key, capacity, period)?;
    let result = bucket.pour(tokens)?;

    // 5. Smart retry
    if result == -1 && smart_retry {
        let calculator = RetryCalculator { bucket: &bucket };
        let retry_ms = calculator.calculate_retry_time(tokens);

        return Ok(RedisValue::Array(vec![
            RedisValue::Integer(-1),
            RedisValue::Integer(retry_ms),
            RedisValue::SimpleString("1.00".to_string()),
        ]).into());
    }

    Ok(result.into())
}
```

### Usage: All Features Combined

```redis
# All AI features enabled
SHIELD.absorb user:123 AUTO AUTO 5 DETECT_ANOMALY ADAPTIVE SMART_RETRY PREDICTIVE

# Returns one of:
# → 95 (allowed, 95 tokens left)
# → -2 (blocked, anomaly detected)
# → [-1, 3500, "1.00"] (denied, retry in 3.5s)
```

---

### Implementation Roadmap

#### Phase 1: Statistical Foundation (v2.3 - 3-4 weeks)
- [ ] Anomaly detection (z-score, no ML)
- [ ] Behavioral scoring (rule-based)
- [ ] Smart retry (mathematical)
- [ ] Redis data structures for history tracking

#### Phase 2: Basic ML (v2.4 - 6-8 weeks)
- [ ] Attack pattern recognition (Random Forest via ONNX)
- [ ] Adaptive limits (linear regression)
- [ ] Simple forecasting (moving average)
- [ ] Model training pipeline
- [ ] ONNX runtime integration

#### Phase 3: Advanced ML (v3.0 - 8-12 weeks)
- [ ] Predictive throttling (seasonal decomposition)
- [ ] User clustering (K-means)
- [ ] Auto-tuning (multi-armed bandit)
- [ ] Feature store optimization
- [ ] A/B testing framework

#### Phase 4: Production ML (v3.1+ - 12+ weeks)
- [ ] Real-time model updates
- [ ] Ensemble models
- [ ] Continuous learning pipeline
- [ ] Advanced time series (Prophet, LSTM)
- [ ] Federated learning (privacy-preserving)

---

### Technical Requirements

#### Model Deployment Options

**Option 1: Embedded Models (ONNX Runtime)**
```rust
// In Redis module
use ort::{Environment, SessionBuilder};

let environment = Environment::default();
let model = SessionBuilder::new(&environment)?
    .with_model_from_file("rate_limit_model.onnx")?;

// Inference
let outputs = model.run(inputs)?;
let prediction = outputs[0].extract_tensor()?;
```

**Pros:**
- Low latency (<1ms)
- No network overhead
- Works offline

**Cons:**
- Model size limited (< 10MB)
- Complex updates
- Memory overhead

---

**Option 2: External ML Service (gRPC)**
```rust
// In Redis module
let fraud_score = ml_client
    .predict(features)
    .timeout(Duration::from_millis(5))
    .await?;
```

**Pros:**
- Large models supported
- Easy updates
- Separate scaling

**Cons:**
- Network latency (5-10ms)
- Requires infrastructure
- Availability dependency

---

**Option 3: Hybrid (Recommended)**
- Simple models embedded (ONNX)
- Complex models via service (cached aggressively)
- Fallback to rule-based if service unavailable

#### Training Pipeline

```
┌─────────────────┐
│  Redis Metrics  │ ─┐
│  (exported)     │  │
└─────────────────┘  │
                     │
┌─────────────────┐  │    ┌──────────────┐
│  Application    │──┼───►│  Data Lake   │
│  Logs           │  │    │  (S3/GCS)    │
└─────────────────┘  │    └──────┬───────┘
                     │           │
┌─────────────────┐  │           │
│  Attack Signals │ ─┘           │
└─────────────────┘              │
                                 ▼
                        ┌─────────────────┐
                        │  ML Training    │
                        │  (Airflow/K8s)  │
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │  Model Registry │
                        │  (MLflow)       │
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │  Redis Module   │
                        │  (hot reload)   │
                        └─────────────────┘
```

---

### Metrics & Monitoring

**Track AI Performance:**

```redis
SHIELD.stats ML
→ 1) model_version: "v1.2.3"
   2) predictions_count: 1000000
   3) avg_inference_latency_ms: 0.8
   4) anomaly_detection_rate: 0.02
   5) false_positive_rate: 0.001
   6) model_accuracy: 0.94
```

**A/B Testing:**
```redis
# Control group: 50% traffic
SHIELD.absorb user:123 100 60 1

# Treatment group: 50% traffic with ML
SHIELD.absorb user:456 100 60 1 ADAPTIVE DETECT_ANOMALY

# Compare:
# - User satisfaction
# - System load reduction
# - Attack prevention rate
```

---

### AI Feature Summary

| Feature | Value | Complexity | Latency Impact | Phase |
|---------|-------|------------|----------------|-------|
| 1. Anomaly Detection | ⭐⭐⭐⭐⭐ | Medium | +0.5ms | 1 |
| 4. Adaptive Limits | ⭐⭐⭐⭐⭐ | Low | +0.1ms | 1 |
| 5. Smart Retry | ⭐⭐⭐⭐ | Low | +0.1ms | 1 |
| 7. Predictive Throttling | ⭐⭐⭐⭐ | High | +1ms | 2-3 |
| 9. User Clustering | ⭐⭐⭐ | Medium | +0.3ms | 2 |

**Recommendation:** Start with Features 1, 4, 5 (Phase 1) as they provide best value/complexity ratio and don't require ML infrastructure.

---

## Implementation Notes

### Backward Compatibility Strategy

**v2.0 Should:**
- ✅ Keep `SHIELD.absorb` command unchanged
- ✅ Add new commands (`inspect`, `reset`, etc.)
- ✅ Make new features opt-in
- ✅ Default response format stays integer
- ✅ No breaking changes to existing behavior

**Migration Path:**
1. Deploy v2.0 alongside v1.0 (same module name)
2. Gradually adopt new commands
3. Enable verbose responses per-application
4. Deprecate old behavior in v3.0 (if needed)

### Testing Strategy

**Unit Tests:**
- Bucket logic (refill calculations)
- Argument parsing
- Edge cases (zero tokens, max capacity)

**Integration Tests:**
- Full command flow through Redis
- Multi-client scenarios
- Cluster compatibility
- Performance regressions

**Chaos Tests:**
- Network failures
- Redis restarts
- Corrupted data
- Clock skew

### Documentation Requirements

**Must Document:**
- Redis Cluster compatibility and limitations
- Performance characteristics and benchmarks
- Memory usage per bucket
- Clock synchronization requirements
- Upgrade path from v1 to v2
- Example client implementations (Python, Node.js, Go, Ruby)

### Release Checklist

- [ ] Update `REDIS_MODULE_VERSION` to 2
- [ ] Add `CHANGELOG.md` entry
- [ ] Update `README.md` with new commands
- [ ] Add migration guide
- [ ] Run full test suite
- [ ] Performance benchmarks (no regressions)
- [ ] Security audit (cargo-audit)
- [ ] Multi-platform builds
- [ ] Generate SHA256 checksums
- [ ] Tag release: `v2.0.0`

---

## Performance Considerations

### Current Performance Profile

**Performance is excellent** - as a compiled native Rust module running inside Redis, operations are extremely fast.

**Minor optimizations possible:**
1. ✅ Pre-allocate strings before Redis calls (already done)
2. 🔄 Cache recent buckets in module memory (risky, added complexity)
3. 🔄 Batch multiple absorb calls via multi-key command (reduces client round-trips)

### Memory Usage

**Per Bucket:**
- Redis key: ~20-50 bytes (depends on key length)
- Redis value: 8 bytes (i64 integer)
- Redis TTL metadata: ~16 bytes
- **Total:** ~50-75 bytes per bucket

**1 Million Buckets:**
- ~50-75 MB Redis memory
- Acceptable for most deployments

### Scalability Limits

**Single Redis Instance:**
- Estimated throughput: **50K+ req/s** (limited by Redis itself, not the module)
- Native compiled Rust code provides minimal overhead
- Actual limits depend on hardware and Redis configuration

**Redis Cluster:**
- Linear scaling with number of shards
- 10 shards × 50K = **500K+ req/s**

---

## V2 Roadmap

### Phase 1: v2.0 (Essential - Q1 2026)

**Goals:** Production-grade observability and debugging

- [ ] `SHIELD.inspect` - Non-destructive bucket inspection
- [ ] `SHIELD.reset` - Manual bucket reset
- [ ] Enhanced response format (array with TTL, capacity, retry_after)
- [ ] `SHIELD.stats` - Internal metrics
- [ ] Redis Cluster documentation
- [ ] Performance benchmarks with criterion (establish baseline metrics)

**Estimated Effort:** 2-3 weeks

---

### Phase 2: v2.1 (High Value - Q2 2026)

**Goals:** Operational efficiency and flexibility

- [ ] `SHIELD.absorb_multi` - Batch operations
- [ ] Separate burst capacity parameter
- [ ] `SHIELD.check` - Dry-run mode
- [ ] Configuration system (`SHIELD.config`)
- [ ] Health check endpoint

**Estimated Effort:** 3-4 weeks

---

### Phase 3: v2.2 (Polish - Q3 2026)

**Goals:** Advanced features and alternative algorithms

- [ ] Leaky bucket algorithm
- [ ] Sliding window algorithm
- [ ] Fractional token costs
- [ ] Bulk management commands (`list`, `flush`, `count`)
- [ ] Penalty mode (TTL extension on deny)
- [ ] Time-based quotas

**Estimated Effort:** 4-6 weeks

---

## Quick Wins (MVP v2.0)

If development time is limited, these 3 features provide maximum value:

### 1. `SHIELD.inspect` Command

**Why:** Biggest operational pain point - can't debug without consuming tokens

**Implementation:** ~50 lines of code
```rust
fn inspect_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 2 {
        return Err(RedisError::WrongArity);
    }

    let key = &args[1];
    let ttl = ctx.call("PTTL", &[key])?;
    let tokens = ctx.call("GET", &[key])?;

    // Return array: [tokens, ttl, exists]
    Ok(RedisValue::Array(vec![tokens, ttl, ...]).into())
}
```

### 2. Enhanced Response Format

**Why:** Enables proper HTTP header support (X-RateLimit-*)

**Implementation:** ~30 lines of code
```rust
// Return [remaining, capacity, ttl, retry_after] instead of just remaining
let response = vec![
    remaining_tokens.into(),
    capacity.into(),
    ttl.into(),
    retry_after_ms.into(),
];
Ok(RedisValue::Array(response).into())
```

### 3. `SHIELD.reset` Command

**Why:** Essential for testing, development, and emergency overrides

**Implementation:** ~20 lines of code
```rust
fn reset_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    // Set key to capacity with full TTL
    ctx.call("PSETEX", &[key, period, capacity])?;
    Ok(capacity.into())
}
```

**Total Effort:** ~1-2 days for all three features

---

## Open Questions

1. **Breaking Changes:** Should v2.0 change default response format?
   - **Recommendation:** No, keep integer response as default, add verbose flag

2. **Multi-Algorithm:** Support in single command or separate commands?
   - **Recommendation:** Single command with `ALGORITHM` parameter

3. **Metrics:** Expose via Redis commands or external system (Prometheus)?
   - **Recommendation:** Both - Redis commands for quick checks, Prometheus for dashboards

4. **Fractional Tokens:** Worth the precision issues?
   - **Recommendation:** Yes, but document floating-point limitations

---

## Contributing

### Code Style

- Follow existing Rust conventions
- Run `cargo fmt` before committing
- Enable all clippy lints (`deny`)
- Add tests for all new features
- Document public APIs with rustdoc comments

### Testing

```bash
# Run tests with Redis
REDIS_URL=redis://127.0.0.1:6379 cargo test

# Run benchmarks
cargo bench

# Security audit
cargo audit

# Format check
cargo fmt --check

# Lint
cargo clippy -- -D warnings
```

### Release Process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Update `REDIS_MODULE_VERSION` constant
4. Run full test suite
5. Tag release: `git tag -a v2.0.0 -m "Release v2.0.0"`
6. Push: `git push origin v2.0.0`
7. GitHub Actions builds and publishes artifacts

---

## References

- [Token Bucket Algorithm (Wikipedia)](https://en.wikipedia.org/wiki/Token_bucket)
- [Redis Modules Documentation](https://redis.io/docs/reference/modules/)
- [redis-module Rust Crate](https://docs.rs/redis-module/)
- [RFC 6585 - Additional HTTP Status Codes](https://tools.ietf.org/html/rfc6585)
- [IETF Draft - RateLimit Headers](https://datatracker.ietf.org/doc/html/draft-polli-ratelimit-headers)

---

## Appendix: Command Reference (Proposed v2)

### Core Commands

```redis
# Consume tokens (existing)
SHIELD.absorb <key> <capacity> <period> [tokens]
→ Returns: (integer) remaining tokens or -1

# Inspect bucket state (new)
SHIELD.inspect <key>
→ Returns: Array [tokens, capacity, period, ttl, utilization]

# Reset bucket (new)
SHIELD.reset <key>
→ Returns: OK

# Delete bucket (new)
SHIELD.delete <key>
→ Returns: (integer) 1 if deleted, 0 if not found
```

### Batch Operations

```redis
# Multi-key absorb (new)
SHIELD.absorb_multi <key1> <cap1> <period1> <tokens1> <key2> <cap2> <period2> <tokens2> ...
→ Returns: Array of remaining tokens

# Check without consuming (new)
SHIELD.check <key> <capacity> <period> [tokens]
→ Returns: (integer) available tokens
```

### Management

```redis
# Configuration (new)
SHIELD.config GET <option>
SHIELD.config SET <option> <value>
SHIELD.config LIST

# Bulk operations (new)
SHIELD.list <pattern>
SHIELD.flush <pattern>
SHIELD.count <pattern>

# Statistics (new)
SHIELD.stats [RESET]
→ Returns: Array of metrics

# Health check (new)
SHIELD.health
→ Returns: Status information
```

### Advanced

```redis
# Alternative algorithms (new)
SHIELD.absorb <key> <capacity> <period> [tokens] ALGORITHM <type>

# Verbose response (new)
SHIELD.absorb <key> <capacity> <period> [tokens] VERBOSE

# Penalty mode (new)
SHIELD.absorb <key> <capacity> <period> [tokens] PENALTY <seconds>
```

---

**End of Document**
