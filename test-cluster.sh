#!/bin/bash
set -e

echo "🔨 Building Redis Shield..."
cargo build --release

echo ""
echo "🚀 Starting Redis Cluster (6 nodes: 3 masters + 3 replicas)..."
docker-compose -f docker-compose.cluster.yml down -v 2>/dev/null || true
docker-compose -f docker-compose.cluster.yml up -d

echo ""
echo "⏳ Waiting for cluster to initialize (30 seconds)..."
sleep 30

echo ""
echo "✅ Cluster Status:"
redis-cli -p 7001 CLUSTER INFO | grep cluster_state

echo ""
echo "📊 Cluster Nodes:"
redis-cli -p 7001 CLUSTER NODES

echo ""
echo "🧪 Testing SHIELD module on all nodes..."
for port in 7001 7002 7003; do
  echo -n "  Port $port: "
  result=$(redis-cli -p $port MODULE LIST | grep -i shield || echo "NOT LOADED")
  if [[ "$result" == "NOT LOADED" ]]; then
    echo "❌ SHIELD not loaded!"
    exit 1
  else
    echo "✅ SHIELD loaded"
  fi
done

echo ""
echo "🔬 Running cluster integration tests..."
REDIS_CLUSTER_URLS="redis://127.0.0.1:7001,redis://127.0.0.1:7002,redis://127.0.0.1:7003" \
  cargo test --features cluster-tests -- --nocapture test_cluster

echo ""
echo "🎯 Manual Test:"
echo "  redis-cli -c -p 7001"
echo "  > SHIELD.absorb user:test 100 60 5"
echo ""
echo "📝 View logs:"
echo "  docker-compose -f docker-compose.cluster.yml logs -f"
echo ""
echo "🧹 Cleanup:"
echo "  docker-compose -f docker-compose.cluster.yml down -v"
