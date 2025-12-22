# Redis Shield - AI Agent Guide

**Redis loadable module** written in **Rust** that implements native rate-limiting algorithms (`SHIELD.absorb`) with token bucket, leaky bucket, fixed window, and sliding window strategies.

## Project Essentials

- **Language:** Rust Edition 2021, MSRV 1.91.1
- **Platforms:** Linux (x86_64, aarch64), macOS (x86_64, aarch64)
- **Core Files:** `src/lib.rs` (command handler), `src/command_parser.rs` (argument parsing), `src/traffic_policy.rs` (policy factory/key management), `src/algorithm/` (rate-limiting implementations)
- **Performance:** 50K-55K req/s throughput, ~19µs latency per operation

## Development Commands

### Building and Testing

```bash
# Build the module
cargo build --release

# Run tests (requires Redis instance)
REDIS_URL=redis://127.0.0.1:6379 cargo test

# Run benchmarks
cargo bench

# Format code (required before commits)
cargo fmt

# Lint (all warnings are errors)
cargo clippy -- -D warnings

# Security audit
cargo audit
```

### Code Style Requirements

- Follow Rust conventions (rustfmt enforced)
- All clippy lints set to `deny` level
- Add tests for all new features
- Document public APIs with rustdoc comments
- Use `#[inline]` for hot path functions
- Prefer integer arithmetic over floating point
- Use static error messages (avoid `.to_string()` allocations)

## Architecture Overview

### Module Structure

```text
src/
├── lib.rs              # Redis command handler / module entry point
├── command_parser.rs   # Argument parsing & validation (algorithm selection, tokens)
├── traffic_policy.rs   # Policy configuration, executor factory, stack key builder
├── algorithm.rs        # Re-exports
└── algorithm/          # Rate limiting implementations
    ├── token_bucket.rs
    ├── leaky_bucket.rs
    ├── fixed_window.rs
    └── sliding_window.rs
```

### Core Components

**Command Handler (`lib.rs`)**

- Entry point: `redis_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult`
- Delegates argument parsing to `command_parser::parse_command_args`
- Uses `traffic_policy::create_executor` to build the requested algorithm
- Executes algorithm via `TrafficPolicyExecutor::execute`; returns remaining tokens (>= 0) or -1 on denial
- Uses Redis allocator in production (`RedisAlloc`), system allocator in tests

**Command Parser (`command_parser.rs`)**

- Validates arity (4–7 args) and parses integers using zero-allocation redis helpers
- Supports optional `ALGORITHM <token_bucket|leaky_bucket|fixed_window|sliding_window>` flag
- Defaults to token_bucket the argument is omitted
- Defaults to 1 token consumption when the argument is omitted
- Exposes `CommandInvocation` struct consumed by the command handler

**Traffic Policy (`traffic_policy.rs`)**

- Defines `PolicyConfig` enum & `TrafficPolicyExecutor` trait
- Stack-based key builder using `ArrayString` (heap fallback for overflow)
- `create_executor` constructs the correct algorithm, passing redis-safe keys

**Algorithms (`src/algorithm/`)**

- `token_bucket.rs`: canonical token bucket implementation (`TokenBucket::new`, `pour`, `fetch_tokens`)
- `leaky_bucket.rs`: leaky bucket that models inflow/outflow with TTL-based leak calculations
- `fixed_window.rs`: fixed window counter with TTL, supports remaining headroom responses
- `sliding_window.rs`: weighted sliding window using serialized state & Redis `TIME`
- `algorithm/mod.rs` re-exports all algorithms for easy use

### Rate Limiting Algorithms

#### Token Bucket

1. Buckets initialize with `capacity` tokens
2. Tokens refill linearly: `refilled = (elapsed / period) * capacity`
3. Consumption is atomic check-and-decrement
4. Uses Redis `PSETEX` (set with TTL) and `PTTL` (get remaining TTL)
5. Period converted from seconds to milliseconds internally

**Example:**

```bash
SHIELD.absorb user123 30 60 13  # 30 capacity, 60s period, consume 13
→ Returns 17 (30 - 13 remaining)
```

#### Leaky Bucket

- Maintains a "water level" that leaks at `capacity/period`
- Rejects additions that overflow capacity
- Uses `PTTL` + elapsed math to leak without timers

#### Fixed Window

- Counts hits in current window (ms)
- TTL-based reset; returns remaining headroom, -1 when full

#### Sliding Window

- Tracks current + previous windows, weights previous window based on elapsed time
- Serializes `start:current:previous` into Redis to maintain state

### Redis Integration

**Commands used:**

- `PSETEX key ms value` - Store token count with TTL
- `PTTL key` - Get remaining milliseconds to calculate refills
- `GET key` - Retrieve current token count
- `TIME` - Sliding window obtains Redis time for millisecond precision

**Data storage:**

- Key: User-provided identifier
- Value: Integer token count
- TTL: Automatically expires when period ends

## Performance Characteristics

### Critical Optimizations Applied

1. **Zero-allocation integer formatting** - Uses `itoa` crate for stack buffers
2. **Integer arithmetic only** - i128 math instead of f64 conversions
3. **Static error messages** - Const strings, no runtime allocations
4. **Function inlining** - `#[inline]` on hot paths
5. **Overflow protection** - `checked_mul()`, `saturating_add()`

### Performance Constraints

- O(1) constant time operations
- Main bottlenecks: Redis Module API calls (`PTTL`, `GET`)
- Network latency if Redis is remote
- Current implementation is near-optimal for architecture

### Benchmarks (November 2025)

| Operation | Latency (P50) | Throughput |
| ----------- | --------------- | ------------ |
| New bucket | ~37 µs | ~27K ops/s |
| Existing bucket | ~19 µs | ~53K ops/s |
| Denied request | ~19 µs | ~53K ops/s |

**DO NOT** add performance optimizations without profiling first.

## Common Development Tasks

### Adding New Features

1. Maintain sub-millisecond latency requirement
2. Avoid heap allocations in hot paths
3. Use `Result<T, RedisError>` for error handling
4. Prefer stack allocations (e.g., `ArrayString`, `itoa::Buffer`) on hot paths
5. Test with `REDIS_URL=redis://127.0.0.1:6379 cargo test`
6. Run benchmarks to verify no performance regression

### Modifying Algorithms

- Token bucket: `algorithm/token_bucket.rs::{new, pour, fetch_tokens}`
- Leaky bucket: `algorithm/leaky_bucket.rs::{new, add, fetch_level}`
- Fixed window: `algorithm/fixed_window.rs::{new, consume, fetch_count, persist_count}`
- Sliding window: `algorithm/sliding_window.rs::{new, consume, load_state, persist_state}`
- All algorithms:
  - Use i128 intermediates where overflow is possible
  - Handle corrupted/missing Redis data defensively
  - Keep TTL calculations in milliseconds
  - Avoid heap allocations inside hot functions; rely on `itoa`, stack buffers

### Testing

- Integration tests remain in `lib.rs` (requires running Redis)
- Unit tests (no Redis needed) cover:
  - `command_parser` (argument parsing, positive integers)
  - `traffic_policy` (key builder & suffixes)
  - `algorithm::sliding_window` helpers (state encoding/usage math)
- Tests use system allocator, production uses RedisAlloc

## Security Considerations

- All inputs validated (positive integers required)
- No buffer overflows (Rust safety guarantees)
- No command injection vectors
- Active dependency scanning via `cargo-audit`
- Handles corrupted Redis data gracefully

## Known Limitations

1. No dry-run mode (can't check without consuming)
2. No bulk operations (one key per command)
3. Returns only integer (no metadata)
4. No built-in observability/metrics

## CI/CD

- `.github/workflows/code_review.yml` - CI pipeline
- `.github/workflows/release.yml` - Multi-platform releases
- `deny.toml` - Security audit configuration

## Design Patterns Used

- Fluent builder pattern (`Bucket::new()`)
- Result type error handling throughout
- Lifetime annotations (`<'a>`) for Redis context validity
- Defensive programming with explicit validation
- Boundary clamping for TTL edge cases

---

When making changes, prioritize:

1. Maintaining performance (profile first)
2. Rust safety and error handling
3. Test coverage for edge cases
4. Code simplicity over cleverness
