# Contributing to Redis Shield

Thank you for your interest in contributing to Redis Shield! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Running Tests](#running-tests)
- [Running Benchmarks](#running-benchmarks)
- [Code Style](#code-style)
- [Making Changes](#making-changes)
- [Submitting Pull Requests](#submitting-pull-requests)
- [Reporting Issues](#reporting-issues)

## Getting Started

Redis Shield is a Redis loadable module written in Rust. Before contributing, make sure you have:

- Rust toolchain 1.95.0 or later
- Redis server installed locally
- [`just`](https://github.com/casey/just) task runner (`cargo install just`)
- Basic understanding of Redis modules and the token bucket algorithm

## Development Setup

1. **Fork and clone the repository:**

```bash
git clone https://github.com/ayarotsky/redis-shield.git
cd redis-shield
```

2. **Install dev tools:**

```bash
just install-tools  # cargo-deny, cargo-audit, cargo-auditable, cargo-pants
```

3. **Build the project:**

```bash
just build-release
```

4. **Start Redis with the module loaded (port 34567):**

```bash
just redis-up
```

5. **Verify the module is loaded:**

```bash
redis-cli -p 34567
127.0.0.1:34567> SHIELD.absorb test 10 60 1
(integer) 9
```

Run `just --list` to see all available recipes.

## Running Tests

`just redis-up` starts Redis with the freshly-built module loaded; `just test`
points at it via `REDIS_URL`.

```bash
just redis-up              # one-time per shell session
just test                  # run the full suite
just test-filter refill    # run tests matching a name filter
just redis-down            # stop the daemonized Redis when done
```

All tests must pass before submitting a pull request.

## Running Benchmarks

Redis Shield includes comprehensive performance benchmarks using [Criterion.rs](https://github.com/bheisler/criterion.rs).

### Quick Start

```bash
just redis-up                       # ensure Redis is up with the module loaded
just bench                          # run all benchmarks
just bench-filter new_bucket        # run a specific benchmark group
open target/criterion/report/index.html  # view HTML reports
```

### Performance Tracking

See **[benches/README.md](benches/README.md)** for complete documentation.

**Expected Performance:**
- New bucket creation: ~37 µs (36-38 µs range)
- Existing bucket (allowed): ~19 µs (18-20 µs range)
- Denied request: ~19 µs (18-20 µs range)
- Throughput: 50,000-55,000 req/s (single connection)

**Important:** Always run benchmarks before and after performance-related changes to verify improvements and prevent regressions.

## Code Style

We follow standard Rust conventions and enforce them through tooling.

### Required Checks

Before committing, run:

```bash
just lint    # fmt --check, clippy -D warnings, cargo deny, cargo pants
just audit   # cargo audit (refreshes the advisory DB first)
```

Use `cargo fmt` directly if you want the formatter to rewrite files (rather than
just check them).

### Style Guidelines

- Follow Rust API guidelines
- Use meaningful variable and function names
- Add rustdoc comments for public APIs
- Keep functions focused and small
- Prefer explicit error handling over panics
- Use `Result<T, RedisError>` for fallible operations

### Performance Guidelines

- Avoid allocations in hot paths
- Use `#[inline]` for small, frequently called functions
- Prefer integer arithmetic over floating point
- Use static error messages (avoid `.to_string()`)
- Profile before optimizing (see benchmarks)
- Document performance-critical sections

### Safety Guidelines

- Validate all user inputs
- Handle Redis edge cases (missing keys, no TTL, corrupted data)
- Use overflow-checked arithmetic (`checked_mul`, `saturating_add`)
- Prefer safe abstractions over `unsafe` code
- Document any `unsafe` blocks with safety invariants

## Making Changes

### Branch Naming

Use descriptive branch names:

- `feature/add-dry-run-mode`
- `fix/handle-negative-ttl`
- `perf/optimize-token-refill`
- `docs/update-usage-examples`

### Commit Messages

Write clear, descriptive commit messages:

```
Add dry-run mode for rate limit checking

- Implement SHIELD.check command that doesn't consume tokens
- Add tests for dry-run behavior
- Update documentation with usage examples

Fixes #123
```

**Format:**
- First line: Short summary (50 chars or less)
- Blank line
- Detailed description (wrap at 72 chars)
- Reference issues/PRs

### Code Organization

- `src/lib.rs` - Redis command handler and module initialization
- `src/bucket.rs` - Token bucket implementation
- Add new modules in `src/` as needed
- Keep related functionality together

## Submitting Pull Requests

1. **Create a feature branch:**

```bash
git checkout -b feature/your-feature-name
```

2. **Make your changes and commit:**

```bash
git add .
git commit -m "Your descriptive commit message"
```

3. **Run all checks:**

```bash
just ci             # full local mirror of .github/workflows/ci.yml
just bench          # if performance-related
```

4. **Push to your fork:**

```bash
git push origin feature/your-feature-name
```

5. **Open a Pull Request:**

- Provide a clear title and description
- Reference related issues
- Describe what changes were made and why
- Include test results and benchmark comparisons (if applicable)
- Add screenshots/examples for new features

### PR Checklist

- [ ] Code follows project style guidelines
- [ ] All tests pass
- [ ] New tests added for new functionality
- [ ] Documentation updated (README, rustdoc, etc.)
- [ ] Benchmarks run (for performance changes)
- [ ] No new compiler warnings
- [ ] Commit messages are clear and descriptive

## Reporting Issues

### Bug Reports

When reporting bugs, include:

- Redis Shield version (commit hash or tag)
- Rust version (`rustc --version`)
- Redis version (`redis-server --version`)
- Operating system and architecture
- Steps to reproduce
- Expected vs actual behavior
- Error messages or logs

### Feature Requests

When requesting features, include:

- Use case and motivation
- Proposed API/interface
- Alternative solutions considered
- Performance implications
- Backwards compatibility concerns

### Security Issues

**Do not** report security vulnerabilities publicly. Instead, email the maintainers directly (see repository for contact information).

## Development Resources

- [Token Bucket Algorithm](https://en.wikipedia.org/wiki/Token_bucket)
- [Redis Modules Documentation](https://redis.io/docs/reference/modules/)
- [redis-module Rust Crate](https://docs.rs/redis-module/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

## Questions?

If you have questions about contributing, feel free to:

- Open a discussion on GitHub
- Ask in pull request comments
- Check existing issues and documentation

---

Thank you for contributing to Redis Shield!
