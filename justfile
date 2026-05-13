# redis-shield — task runner
# Run `just` (or `just --list`) to see available recipes.
#
# Install just: cargo install just
# Install dev tools: just install-tools

set shell := ["bash", "-cu"]

REDIS_PORT := "34567"
REDIS_URL  := "redis://127.0.0.1:" + REDIS_PORT + "/1"
MODULE_LIB := if os() == "macos" { "libredis_shield.dylib" } else { "libredis_shield.so" }

# List all available recipes
default:
    @just --list

# ── Lint ──────────────────────────────────────────────────────────────────────

# Format check, clippy (warnings as errors), cargo-deny, and cargo-pants.
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo deny check
    cargo pants

# ── Build ─────────────────────────────────────────────────────────────────────

# Build the module (debug) at target/debug/libredis_shield.{so,dylib}.
build:
    cargo build

# Build the module (release) with auditable metadata embedded.
build-release:
    cargo auditable build --release

# ── Audit ─────────────────────────────────────────────────────────────────────

# Refresh the advisory DB then run cargo-audit.
audit:
    rm -rf ~/.cargo/advisory-db
    cargo audit

# ── Redis Helpers ─────────────────────────────────────────────────────────────

# Build the module and start a daemonized Redis on REDIS_PORT with it loaded.
# Idempotent: no-op if Redis is already up on that port.
redis-up: build
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

# ── Testing ───────────────────────────────────────────────────────────────────

# Run the full test suite (expects `just redis-up` first, or external Redis with module).
test:
    REDIS_URL={{REDIS_URL}} cargo test

# ── Benchmarks ────────────────────────────────────────────────────────────────

# Run all Criterion benchmarks (expects `just redis-up` first).
bench:
    REDIS_URL={{REDIS_URL}} cargo bench

# Run benchmarks matching a name filter — e.g. `just bench-filter new_bucket`
bench-filter filter:
    REDIS_URL={{REDIS_URL}} cargo bench -- {{ filter }}

# ── Composite ─────────────────────────────────────────────────────────────────

# Fast pre-commit gate: lint + tests (assumes `just redis-up` already ran).
check: lint test

# Full CI pipeline — mirrors .github/workflows/ci.yml. Run before opening a PR.
ci: lint build audit redis-up test redis-down
    @echo "CI pipeline passed locally."

# ── Tool Installation ─────────────────────────────────────────────────────────

# Install tools required for development and CI (idempotent).
install-tools:
    cargo install --locked cargo-deny
    cargo install --locked cargo-audit
    cargo install --locked cargo-auditable
    cargo install --locked cargo-pants
