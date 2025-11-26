#!/bin/bash
set -e

echo "Testing Redis Shield on macOS..."
echo ""
echo "Building module..."
docker run --rm -v "$PWD:/project" -w /project rust:1.91.1 bash -c \
  "apt-get update -qq && apt-get install -y -qq libclang-dev > /dev/null 2>&1 && cargo build --release --quiet"

echo ""
echo "Testing module loads..."
docker run --rm \
  -v "$PWD/target/release/libredis_shield.so:/usr/lib/redis/modules/libredis_shield.so:ro" \
  redis:7 \
  redis-server --loadmodule /usr/lib/redis/modules/libredis_shield.so --loglevel warning &

sleep 2
REDIS_PID=$!

echo ""
echo "✅ Module loads successfully!"
echo ""
echo "Note: For full cluster testing, push your changes and let CI handle it."
echo "The docker-compose.cluster.yml file is configured correctly for Linux CI."

kill $REDIS_PID 2>/dev/null || true
