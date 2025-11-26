# Redis Shield Benchmarks

This directory contains performance benchmarks for the Redis Shield rate limiting module using [Criterion.rs](https://github.com/bheisler/criterion.rs).

## Prerequisites

1. **Redis Server**: You must have a Redis server running with the Redis Shield module loaded
2. **Rust**: Rust 1.91.1 or later
3. **Environment Variable**: Set `REDIS_URL` to your Redis instance (defaults to `redis://127.0.0.1:6379`)

## Running Benchmarks

### Run All Benchmarks

```bash
# Set Redis URL (optional, defaults to localhost)
export REDIS_URL=redis://127.0.0.1:6379

# Run all benchmarks
cargo bench
```

### Run Specific Benchmark Groups

```bash
# Run only new bucket creation benchmarks
cargo bench --bench shield_benchmarks -- new_bucket

# Run only existing bucket benchmarks
cargo bench --bench shield_benchmarks -- existing_bucket

# Run only denied request benchmarks
cargo bench --bench shield_benchmarks -- denied_requests
```

### Run with Custom Sample Size

```bash
cargo bench -- --quick
```

## Output and Reports

### Terminal Output

Criterion provides detailed statistical output in the terminal:

```
new_bucket/capacity/100 time:   [125.43 µs 127.91 µs 130.89 µs]
                        thrpt:  [7.6399 Kelem/s 7.8174 Kelem/s 7.9714 Kelem/s]
```

### HTML Reports

Detailed HTML reports are generated in `target/criterion/`:

```bash
# Open the main report
open target/criterion/report/index.html

# Or view specific benchmark
open target/criterion/new_bucket/capacity/100/report/index.html
```

Reports include:
- Performance graphs
- Statistical analysis
- Comparison with previous runs
- Regression detection

## Interpreting Results

### Expected Performance

Based on the architecture (native Rust module inside Redis) and optimizations applied (zero-allocation integer formatting, integer arithmetic, static error messages, function inlining):

- **New bucket creation**: ~37 µs (includes Redis key creation + TTL set)
- **Existing bucket (allowed)**: ~19 µs (GET + refill calc + PSETEX)
- **Denied request**: ~19 µs (GET + TTL check only)
- **Throughput**: 50,000-55,000 requests/second (single connection)

### Performance Baselines

| Operation | P50 (Median) | Expected Range |
|-----------|------------|----------------|
| New bucket | ~37 µs | 36-38 µs |
| Existing (allowed) | ~19 µs | 18-20 µs |
| Denied | ~19 µs | 18-20 µs |

**Historical improvement**: These results are ~3.5x faster than the original baseline (130µs → 37µs for new buckets, 80µs → 19µs for existing buckets) due to performance optimizations applied in November 2025.

*Note: Actual performance varies by hardware, Redis configuration, and network latency*

### What to Look For

✅ **Good Performance Indicators:**
- Consistent timings across different capacities
- Denied requests ≈ or faster than allowed requests
- Linear scaling with token consumption
- Minimal variance in repeated runs

⚠️ **Performance Issues:**
- High variance (>20% coefficient of variation)
- Non-linear scaling with parameters
- Regression compared to previous runs
- Significantly slower than baselines above

## Continuous Benchmarking

### Regression Detection

Criterion automatically detects performance regressions:

```bash
# Run benchmarks and save baseline
cargo bench -- --save-baseline main

# After changes, compare against baseline
cargo bench -- --baseline main
```

## Benchmarking Best Practices

### 1. Consistent Environment

- Close unnecessary applications
- Run on dedicated hardware (not shared VM)
- Disable CPU frequency scaling:
  ```bash
  # Linux
  sudo cpupower frequency-set --governor performance

  # macOS - use consistent power mode
  ```

### 2. Redis Configuration

- Use dedicated Redis instance for benchmarking
- Disable persistence (RDB/AOF) to reduce variance
- Ensure sufficient memory (no eviction during tests)

```bash
# redis.conf
save ""
appendonly no
maxmemory-policy noeviction
```
