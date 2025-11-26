# Redis Shield

![Build Status](https://github.com/ayarotsky/redis-shield/actions/workflows/code_review.yml/badge.svg?branch=main)

Redis Shield is a loadable Redis module that implements the
[token bucket algorithm](https://en.wikipedia.org/wiki/Token_bucket)
to do rate limiting as a native command.

## Algorithm

The token bucket algorithm is based on an analogy of a fixed capacity bucket into which
tokens are added at a fixed rate. When a request is to be checked for conformance to
the defined limits, the bucket is inspected to see if it contains sufficient tokens
at that time. If so, the appropriate number of tokens, e.g. equivalent to the number
of HTTP requests, are removed, and the request is passed.

The request does not conform if there are insufficient tokens in the bucket.

## Install

Clone and build the project from source.

    $ git clone https://github.com/ayarotsky/redis-shield.git
    $ cd redis-shield
    $ cargo build --release
    $ # extension will be **.dylib** instead of **.so** for Mac releases
    $ cp target/release/libredis_shield.so /path/to/modules/

Run redis-server pointing to the newly built module:

    redis-server --loadmodule /path/to/modules/libredis_shield.so

**Or** add the following to a `redis.conf` file:

    loadmodule /path/to/modules/libredis_shield.so

## Usage

    SHIELD.absorb <key> <capacity> <period> [<tokens>]

Where `key` is a unique bucket identifier. Examples:

* User ID
* Request's IP address

For example:

    SHIELD.absorb ip-127.0.0.1 30 60 11
                    ▲           ▲  ▲ ▲
                    |           |  | └─── take 11 token (default is 1 if omitted)
                    |           |  └───── 60 seconds
                    |           └──────── 30 tokens
                    └──────────────────── key "ip-127.0.0.1"

The command responds with the number of tokens left in the bucket.
`-1` is returned when the bucket is overflown.

    127.0.0.1:6379> SHIELD.absorb user123 30 60 13
    (integer) 17
    127.0.0.1:6379> SHIELD.absorb user123 30 60 13
    (integer) 4
    127.0.0.1:6379> SHIELD.absorb user123 30 60 13
    (integer) -1

## Development

### Running Tests

```bash
REDIS_URL=redis://127.0.0.1:6379 cargo test
```

### Running Benchmarks

Redis Shield includes comprehensive performance benchmarks using [Criterion.rs](https://github.com/bheisler/criterion.rs).

#### Quick Start

```bash
# Ensure Redis is running with the module loaded
export REDIS_URL=redis://127.0.0.1:6379

# Run all benchmarks
cargo bench

# Run specific benchmark group
cargo bench -- new_bucket

# Generate HTML reports (saved to target/criterion/)
cargo bench
open target/criterion/report/index.html
```

#### Performance Tracking

See **[benches/README.md](benches/README.md)** for complete documentation.

**Expected Performance:**
- New bucket creation: ~47 µs (45-50 µs range)
- Existing bucket (allowed): ~25 µs (23-27 µs range)
- Denied request: ~23 µs (21-25 µs range)
- Throughput: 35,000-45,000 req/s (single connection)

*Performance improved ~3x through optimizations: zero-allocation integer formatting, integer arithmetic, static error messages, and function inlining.*

## License

This is free software under the terms of MIT the license (see the file
`LICENSE` for details).
