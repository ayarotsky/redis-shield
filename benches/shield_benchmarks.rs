use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use redis::{Commands, Connection};
use std::env;
use std::hint::black_box;

const REDIS_COMMAND: &str = "SHIELD.absorb";

/// Establishes a connection to Redis using the REDIS_URL environment variable.
///
/// # Panics
/// Panics if REDIS_URL is not set or connection fails.
fn establish_connection() -> Connection {
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let client = redis::Client::open(redis_url)
        .expect("Failed to create Redis client");
    client.get_connection()
        .expect("Failed to connect to Redis")
}

/// Helper function to execute SHIELD.absorb command
fn shield_absorb(
    con: &mut Connection,
    key: &str,
    capacity: i64,
    period: i64,
    tokens: Option<i64>,
) -> redis::RedisResult<i64> {
    let mut cmd = redis::cmd(REDIS_COMMAND);
    cmd.arg(key).arg(capacity).arg(period);
    if let Some(t) = tokens {
        cmd.arg(t);
    }
    cmd.query(con)
}

/// Helper function to clean up test keys
fn cleanup_key(con: &mut Connection, key: &str) {
    let _: Result<(), redis::RedisError> = con.del(key);
}

/// Benchmark: Creating new buckets
fn bench_new_bucket(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("new_bucket");

    // Benchmark with different capacity sizes
    for capacity in [10, 100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("capacity", capacity),
            capacity,
            |b, &cap| {
                let key = format!("bench:new:{}", cap);
                b.iter(|| {
                    cleanup_key(&mut con, &key);
                    shield_absorb(
                        &mut con,
                        black_box(&key),
                        black_box(cap),
                        black_box(60),
                        Some(black_box(1)),
                    )
                });
            },
        );
    }
    group.finish();
}

/// Benchmark: Absorbing from existing buckets (allowed case)
fn bench_existing_bucket_allowed(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("existing_bucket_allowed");

    for capacity in [10, 100, 1000, 10000].iter() {
        let key = format!("bench:existing:{}", capacity);

        // Pre-create the bucket
        cleanup_key(&mut con, &key);
        shield_absorb(&mut con, &key, *capacity, 60, Some(1))
            .expect("Failed to create bucket");

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("capacity", capacity),
            capacity,
            |b, &cap| {
                b.iter(|| {
                    shield_absorb(
                        &mut con,
                        black_box(&format!("bench:existing:{}", cap)),
                        black_box(cap),
                        black_box(60),
                        Some(black_box(1)),
                    )
                });
            },
        );

        cleanup_key(&mut con, &key);
    }
    group.finish();
}

/// Benchmark: Denied requests (insufficient tokens)
fn bench_denied_requests(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("denied_requests");

    let key = "bench:denied";

    // Pre-create bucket with very low capacity
    cleanup_key(&mut con, key);
    shield_absorb(&mut con, key, 1, 60, Some(1))
        .expect("Failed to create bucket");

    group.throughput(Throughput::Elements(1));
    group.bench_function("insufficient_tokens", |b| {
        b.iter(|| {
            // This should return -1 (denied)
            shield_absorb(
                &mut con,
                black_box(key),
                black_box(1),
                black_box(60),
                Some(black_box(1)),
            )
        });
    });

    cleanup_key(&mut con, key);
    group.finish();
}

/// Benchmark: Varying token consumption amounts
fn bench_token_consumption(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("token_consumption");

    for tokens in [1, 5, 10, 50, 100].iter() {
        let key = format!("bench:tokens:{}", tokens);

        group.throughput(Throughput::Elements(*tokens as u64));
        group.bench_with_input(
            BenchmarkId::new("tokens", tokens),
            tokens,
            |b, &tok| {
                b.iter(|| {
                    let key = format!("bench:tokens:{}", tok);
                    // Reset bucket each iteration
                    cleanup_key(&mut con, &key);
                    shield_absorb(
                        &mut con,
                        black_box(&key),
                        black_box(1000),
                        black_box(60),
                        Some(black_box(tok)),
                    )
                });
            },
        );

        cleanup_key(&mut con, &key);
    }
    group.finish();
}

/// Benchmark: Different period durations
fn bench_period_variations(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("period_variations");

    // Test with periods from 1 second to 1 hour
    for period in [1, 10, 60, 300, 3600].iter() {
        let key = format!("bench:period:{}", period);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("period_seconds", period),
            period,
            |b, &per| {
                b.iter(|| {
                    let key = format!("bench:period:{}", per);
                    cleanup_key(&mut con, &key);
                    shield_absorb(
                        &mut con,
                        black_box(&key),
                        black_box(100),
                        black_box(per),
                        Some(black_box(1)),
                    )
                });
            },
        );

        cleanup_key(&mut con, &key);
    }
    group.finish();
}

/// Benchmark: Concurrent access simulation (sequential for single connection)
fn bench_high_frequency(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("high_frequency");

    let key = "bench:highfreq";

    // Pre-create bucket with high capacity
    cleanup_key(&mut con, key);
    shield_absorb(&mut con, key, 10000, 60, Some(1))
        .expect("Failed to create bucket");

    group.throughput(Throughput::Elements(100));
    group.bench_function("100_sequential_requests", |b| {
        b.iter(|| {
            // Reset bucket
            cleanup_key(&mut con, key);
            shield_absorb(&mut con, key, 10000, 60, Some(1))
                .expect("Failed to create bucket");

            // Make 100 requests
            for _ in 0..100 {
                shield_absorb(
                    &mut con,
                    black_box(key),
                    black_box(10000),
                    black_box(60),
                    Some(black_box(1)),
                ).expect("Request failed");
            }
        });
    });

    cleanup_key(&mut con, key);
    group.finish();
}

/// Benchmark: Bucket refill calculation (test with aged buckets)
fn bench_refill_calculation(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("refill_calculation");

    let key = "bench:refill";

    // Create a bucket and exhaust it
    cleanup_key(&mut con, key);
    shield_absorb(&mut con, key, 100, 1, Some(100))
        .expect("Failed to create bucket");

    group.throughput(Throughput::Elements(1));
    group.bench_function("after_partial_refill", |b| {
        b.iter(|| {
            // Sleep briefly to allow some refill (in real-world, time passes naturally)
            // This tests the refill calculation logic
            std::thread::sleep(std::time::Duration::from_millis(100));

            shield_absorb(
                &mut con,
                black_box(key),
                black_box(100),
                black_box(1),
                Some(black_box(1)),
            )
        });
    });

    cleanup_key(&mut con, key);
    group.finish();
}

/// Benchmark: Edge case - minimum values
fn bench_edge_cases(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("edge_cases");

    // Minimum capacity (1 token)
    group.bench_function("min_capacity", |b| {
        let key = "bench:edge:min_cap";
        b.iter(|| {
            cleanup_key(&mut con, key);
            shield_absorb(
                &mut con,
                black_box(key),
                black_box(1),
                black_box(1),
                Some(black_box(1)),
            )
        });
        cleanup_key(&mut con, key);
    });

    // Very large capacity
    group.bench_function("max_capacity", |b| {
        let key = "bench:edge:max_cap";
        b.iter(|| {
            cleanup_key(&mut con, key);
            shield_absorb(
                &mut con,
                black_box(key),
                black_box(1_000_000),
                black_box(60),
                Some(black_box(1)),
            )
        });
        cleanup_key(&mut con, key);
    });

    // Very long period
    group.bench_function("long_period", |b| {
        let key = "bench:edge:long_period";
        b.iter(|| {
            cleanup_key(&mut con, key);
            shield_absorb(
                &mut con,
                black_box(key),
                black_box(100),
                black_box(86400), // 1 day
                Some(black_box(1)),
            )
        });
        cleanup_key(&mut con, key);
    });

    group.finish();
}

/// Benchmark: Different key lengths
fn bench_key_lengths(c: &mut Criterion) {
    let mut con = establish_connection();
    let mut group = c.benchmark_group("key_lengths");

    let very_long_key = "x".repeat(256);
    let keys = vec![
        ("short", "x"),
        ("medium", "user:12345"),
        ("long", "api:v2:endpoint:/very/long/path:user:12345:session:abcdef"),
        ("very_long", very_long_key.as_str()),
    ];

    for (name, key) in keys {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("key_length", name),
            &key,
            |b, key| {
                b.iter(|| {
                    cleanup_key(&mut con, key);
                    shield_absorb(
                        &mut con,
                        black_box(key),
                        black_box(100),
                        black_box(60),
                        Some(black_box(1)),
                    )
                });
            },
        );
        cleanup_key(&mut con, key);
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_new_bucket,
    bench_existing_bucket_allowed,
    bench_denied_requests,
    bench_token_consumption,
    bench_period_variations,
    bench_high_frequency,
    bench_refill_calculation,
    bench_edge_cases,
    bench_key_lengths
);

criterion_main!(benches);
