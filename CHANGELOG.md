# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-07-18

### Changed

- Bump rust to v1.88.0
- Improved test coverage
- Improved documentation and code comments

### Added

- ARM64 support (aarch64-apple-darwin, aarch64-unknown-linux-gnu) in release builds
- SHA256 checksums for release artifacts
- Cargo tools caching in CI for faster builds
- Enhanced error handling and Redis connection resilience tests

### Fixed

- Improved Redis startup reliability in CI with proper health checks

## [0.4.2] - 2025-06-10

### Changed

- Bump rust to v1.87.0
- Bump redis to v0.32.0

## [0.4.1] - 2024-12-10

### Fixed

- [RUSTSEC-2024-0421](https://rustsec.org/advisories/RUSTSEC-2024-0421)
- [RUSTSEC-2024-0407](https://rustsec.org/advisories/RUSTSEC-2024-0407)

## [0.4.0] - 2024-11-17

### Added

- Initial release of `redis-shield`
- `SHIELD.absorb` redis command that implements rate limiting using the token bucket algorithm
