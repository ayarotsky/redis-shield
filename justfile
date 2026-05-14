# redis-shield — task runner
# Run `just` (or `just --list`) to see available recipes.
#
# Install just: cargo install just

set shell := ["bash", "-cu"]

REDIS_PORT := "34567"
REDIS_URL  := "redis://127.0.0.1:" + REDIS_PORT + "/1"
MODULE_LIB := if os() == "macos" { "libredis_shield.dylib" } else { "libredis_shield.so" }

# List all available recipes
default:
    @just --list

# ── Lint ──────────────────────────────────────────────────────────────────────

# Format check, clippy (warnings as errors), and cargo-deny.
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo deny check

# ── Build ─────────────────────────────────────────────────────────────────────

# Build the module (release).
build-release:
    cargo build --release

# ── Redis Helpers ─────────────────────────────────────────────────────────────

# Start a daemonized Redis on REDIS_PORT with it loaded.
# Idempotent: no-op if Redis is already up on that port.
redis-up:
    if redis-cli -p {{REDIS_PORT}} ping >/dev/null 2>&1; then \
      echo "Redis already running on port {{REDIS_PORT}}"; \
    else \
      redis-server \
        --port {{REDIS_PORT}} \
        --loadmodule target/debug/{{MODULE_LIB}} \
        --daemonize yes; \
      sleep 2; \
      redis-cli -p {{REDIS_PORT}} ping; \
    fi

# Stop the daemonized Redis started by `redis-up`.
redis-down:
    -redis-cli -p {{REDIS_PORT}} shutdown nosave

# ── Valkey Helpers ────────────────────────────────────────────────────────────

# Start a daemonized Valkey on REDIS_PORT with it loaded.
# Idempotent: no-op if Valkey is already up on that port.
valkey-up:
    if valkey-cli -p {{REDIS_PORT}} ping >/dev/null 2>&1; then \
      echo "Valkey already running on port {{REDIS_PORT}}"; \
    else \
      valkey-server \
        --port {{REDIS_PORT}} \
        --loadmodule target/debug/{{MODULE_LIB}} \
        --daemonize yes; \
      sleep 2; \
      valkey-cli -p {{REDIS_PORT}} ping; \
    fi

# Stop the daemonized Valkey started by `valkey-up`.
valkey-down:
    -valkey-cli -p {{REDIS_PORT}} shutdown nosave

# ── Testing ───────────────────────────────────────────────────────────────────

# Run the full test suite (expects `just redis-up` first, or external Redis with module).
test:
    REDIS_URL={{REDIS_URL}} cargo test

# Fast pre-commit gate: lint + tests (assumes `just redis-up` already ran).
check: lint test

# ── Benchmarks ────────────────────────────────────────────────────────────────

# Run all Criterion benchmarks (expects `just redis-up` first).
bench:
    REDIS_URL={{REDIS_URL}} cargo bench

# Run benchmarks matching a name filter — e.g. `just bench-filter new_bucket`
bench-filter filter:
    REDIS_URL={{REDIS_URL}} cargo bench -- {{ filter }}
