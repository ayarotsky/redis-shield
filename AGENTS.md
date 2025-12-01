# Redis Shield - AI Agent Guide

**Redis loadable module** written in **Rust** that implements **token bucket rate limiting** as a native Redis command (`SHIELD.absorb`).

## Project Essentials

- **Language:** Rust Edition 2021, MSRV 1.91.1
- **Platforms:** Linux (x86_64, aarch64), macOS (x86_64, aarch64)
- **Core Files:** `src/lib.rs` (command handler), `src/bucket.rs` (token bucket algorithm)
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

```
src/
├── lib.rs          # Redis command handler, entry point
└── bucket.rs       # Token bucket implementation
```

### Core Components

**Command Handler (`lib.rs`)**
- Entry point: `redis_command(ctx: &Context, args: Vec<RedisString>) -> RedisResult`
- Validates 4-5 arguments: `SHIELD.absorb <key> <capacity> <period> [tokens]`
- Uses Redis allocator in production (`RedisAlloc`), system allocator in tests
- Returns remaining tokens (>= 0) or -1 on denial

**Token Bucket (`bucket.rs`)**
```rust
pub struct Bucket<'a> {
    pub key: &'a RedisString,
    pub capacity: i64,
    pub period: i64,        // Milliseconds internally
    pub tokens: i64,
    ctx: &'a Context,
}
```

Key methods:
- `Bucket::new()` - Creates/retrieves bucket, calculates refills
- `Bucket::pour(tokens)` - Consumes tokens if available, returns remaining or -1
- `fetch_tokens()` - Uses `PTTL` to calculate elapsed time and refill tokens

### Token Bucket Algorithm

1. Buckets initialize with `capacity` tokens
2. Tokens refill linearly: `refilled = (elapsed / period) * capacity`
3. Consumption is atomic check-and-decrement
4. Uses Redis `PSETEX` (set with TTL) and `PTTL` (get remaining TTL)
5. Period converted from seconds to milliseconds internally

**Example:**
```
SHIELD.absorb user123 30 60 13  # 30 capacity, 60s period, consume 13
→ Returns 17 (30 - 13 remaining)
```

### Redis Integration

**Commands used:**
- `PSETEX key ms value` - Store token count with TTL
- `PTTL key` - Get remaining milliseconds to calculate refills
- `GET key` - Retrieve current token count

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
|-----------|---------------|------------|
| New bucket | ~37 µs | ~27K ops/s |
| Existing bucket | ~19 µs | ~53K ops/s |
| Denied request | ~19 µs | ~53K ops/s |

**DO NOT** add performance optimizations without profiling first.

## Common Development Tasks

### Adding New Features

1. Maintain sub-millisecond latency requirement
2. Avoid heap allocations in hot paths
3. Use `Result<T, RedisError>` for error handling
4. Test with `REDIS_URL=redis://127.0.0.1:6379 cargo test`
5. Run benchmarks to verify no performance regression

### Modifying Token Bucket Logic

- Core algorithm in `bucket.rs::fetch_tokens()` and `bucket.rs::pour()`
- Uses i128 intermediate calculations for precision
- TTL-based timing system (milliseconds)
- Handle edge cases: missing keys, no TTL, corrupted data

### Testing

- Integration tests in `lib.rs` (~660 lines, 26 tests)
- Requires running Redis instance
- Tests use system allocator, production uses RedisAlloc
- Mock Redis context with `redis_module::test::RedisContext`

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
