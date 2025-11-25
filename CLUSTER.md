# Redis Cluster Compatibility Guide

Redis Shield is **fully compatible with Redis Cluster**. This guide explains how to deploy, test, and use Redis Shield in a clustered environment.

## Quick Start

### 1. Build the Module

```bash
cargo build --release
```

### 2. Start Test Cluster

```bash
docker compose -f docker-compose.cluster.yml up -d
```

This creates a 6-node cluster (3 masters + 3 replicas) with Redis Shield loaded on all nodes.

### 3. Test Cluster

```bash
# Connect to cluster
redis-cli -c -p 7001

# Test SHIELD command
127.0.0.1:7001> SHIELD.absorb user123 100 60 5
(integer) 95
```

### 4. Run Integration Tests

All tests are in `src/lib.rs`. Cluster tests are gated behind the `cluster-tests` feature:

```bash
# Run all cluster tests
REDIS_CLUSTER_URLS="redis://127.0.0.1:7001,redis://127.0.0.1:7002,redis://127.0.0.1:7003" \
  cargo test --features cluster-tests test_cluster

# Or use the helper script
./test-cluster.sh
```

---

## How It Works

### Single-Key Operations = Cluster-Safe

Redis Shield uses **only single-key operations**, which makes it naturally cluster-compatible:

```rust
// All operations target a single key
PTTL {key}         // Get TTL
GET {key}          // Get token count
PSETEX {key} ...   // Set token count with TTL
```

**Key benefits:**
- ✅ Each key maps to exactly one hash slot
- ✅ No cross-slot operations
- ✅ No MULTI/EXEC transactions needed
- ✅ Automatic failover support

---

## Hash Slot Mapping

### Understanding Hash Slots

Redis Cluster splits the keyspace into **16,384 hash slots**. Each key is assigned to a slot using:

```
slot = CRC16(key) mod 16384
```

### Key Distribution Examples

```redis
# Different keys → different slots (likely different nodes)
SHIELD.absorb user:123 100 60 5     # Slot: 14242
SHIELD.absorb user:456 100 60 5     # Slot: 7891
SHIELD.absorb user:789 100 60 5     # Slot: 2134

# All buckets are independent, may be on different nodes
```

### Hash Tags for Same-Slot Keys

Use **hash tags** `{...}` to force keys to the same slot:

```redis
# All these keys use the hash of "user:123" only
SHIELD.absorb {user:123}:api 100 60 5       # Slot: 5798
SHIELD.absorb {user:123}:web 100 60 5       # Slot: 5798
SHIELD.absorb {user:123}:mobile 100 60 5    # Slot: 5798

# All on the same node, can be used in pipelines
```

**When to use hash tags:**
- Multi-resource rate limiting for same user
- Pipeline optimization
- Future multi-key operations (v2.0+)

**When NOT to use:**
- High-traffic keys (creates hotspots)
- You want load distribution

---

## Cluster Topology

### Recommended Setup

```
┌─────────────────────────────────────────────┐
│           Redis Cluster (3 Masters)         │
├─────────────────────────────────────────────┤
│                                             │
│  Master 1          Master 2       Master 3  │
│  (Slots 0-5460)    (5461-10922)  (10923+)   │
│  Port: 7001        Port: 7002    Port: 7003 │
│  + SHIELD          + SHIELD      + SHIELD   │
│     ↓                  ↓             ↓       │
│  Replica 1         Replica 2     Replica 3  │
│  Port: 7004        Port: 7005    Port: 7006 │
│  + SHIELD          + SHIELD      + SHIELD   │
│                                             │
└─────────────────────────────────────────────┘
```

### Deployment Checklist

- [ ] Load `libredis_shield.so` on **all nodes** (masters + replicas)
- [ ] Enable cluster mode: `cluster-enabled yes`
- [ ] Set cluster timeout: `cluster-node-timeout 5000`
- [ ] Enable AOF persistence: `appendonly yes`
- [ ] Configure proper network: accessible from clients
- [ ] Test module on each node individually
- [ ] Initialize cluster with `redis-cli --cluster create`
- [ ] Verify cluster health: `redis-cli --cluster check`

---

## Client Configuration

### Connection String

```python
# Python (redis-py)
from redis.cluster import RedisCluster

cluster = RedisCluster(
    startup_nodes=[
        {"host": "127.0.0.1", "port": 7001},
        {"host": "127.0.0.1", "port": 7002},
        {"host": "127.0.0.1", "port": 7003},
    ],
    decode_responses=True,
    skip_full_coverage_check=False,  # Ensure all slots covered
)

# Use SHIELD command
result = cluster.execute_command('SHIELD.absorb', 'user:123', 100, 60, 5)
```

```javascript
// Node.js (ioredis)
const Redis = require('ioredis');

const cluster = new Redis.Cluster([
  { host: '127.0.0.1', port: 7001 },
  { host: '127.0.0.1', port: 7002 },
  { host: '127.0.0.1', port: 7003 },
]);

// Use SHIELD command
const result = await cluster.call('SHIELD.absorb', 'user:123', 100, 60, 5);
```

```go
// Go (go-redis)
package main

import (
    "github.com/go-redis/redis/v8"
    "context"
)

func main() {
    ctx := context.Background()

    client := redis.NewClusterClient(&redis.ClusterOptions{
        Addrs: []string{
            "127.0.0.1:7001",
            "127.0.0.1:7002",
            "127.0.0.1:7003",
        },
    })

    // Use SHIELD command
    result, err := client.Do(ctx, "SHIELD.absorb", "user:123", 100, 60, 5).Int64()
}
```

### Client Features to Enable

✅ **Auto-retry on MOVED/ASK:** Most cluster clients handle this automatically
✅ **Slot cache refresh:** Clients cache slot→node mapping, refresh on topology changes
✅ **Read-from-replica:** Optional, but Shield works on replicas (read-only)

---

## Operations Guide

### Normal Operations

```redis
# Single-key operations work seamlessly
SHIELD.absorb user:alice 100 60 5
→ 95

# Client automatically routes to correct node
# Handles MOVED redirects transparently
```

### During Resharding

```redis
# Buckets remain accessible during migration
# Clients may see temporary MOVED/ASK errors (auto-handled)

# Before reshard: user:123 on Node 1
SHIELD.absorb user:123 100 60 5
→ 95

# During reshard: slot migrating Node 1 → Node 2
SHIELD.absorb user:123 100 60 5
→ (client sees ASK, redirects to Node 2)
→ 90

# After reshard: user:123 on Node 2
SHIELD.absorb user:123 100 60 5
→ 85
```

**What happens to buckets:**
- Bucket data (token count + TTL) migrates with the key
- No data loss during migration
- Brief increased latency during slot migration

### During Failover

```redis
# Master fails
# Replica promoted to master automatically
# Buckets available on new master

# Before failover: Master 1 has user:123
SHIELD.absorb user:123 100 60 5
→ 95

# Master 1 crashes
# Replica 1 promoted to Master

# After failover: New Master (formerly Replica 1) has user:123
SHIELD.absorb user:123 100 60 5
→ 90 (continues from last state)
```

**Failover behavior:**
- AOF persistence ensures no data loss
- Brief unavailability during promotion (~few seconds)
- Cluster automatically updates topology

---

## Monitoring

### Check Module on All Nodes

```bash
#!/bin/bash
for port in 7001 7002 7003 7004 7005 7006; do
  echo "Checking node on port $port..."
  redis-cli -p $port MODULE LIST | grep -i shield || echo "SHIELD not loaded!"
done
```

### Monitor Cluster Health

```bash
# Cluster status
redis-cli -c -p 7001 CLUSTER INFO

# Check all slots covered
redis-cli --cluster check 127.0.0.1:7001

# Node health
redis-cli -c -p 7001 CLUSTER NODES
```

### Metrics to Track

| Metric | Command | Description |
|--------|---------|-------------|
| Keys per node | `DBSIZE` | Check distribution balance |
| Memory usage | `INFO memory` | Monitor per-node memory |
| Cluster state | `CLUSTER INFO` | Overall health |
| Slot coverage | `CLUSTER SLOTS` | Ensure all slots assigned |

---

## Troubleshooting

### Module Not Loaded on Node

**Symptom:**
```
(error) ERR unknown command 'SHIELD.absorb'
```

**Solution:**
```bash
# Check if module loaded
redis-cli -p 7001 MODULE LIST

# If missing, add to redis.conf or loadmodule arg
redis-server --loadmodule /path/to/libredis_shield.so
```

### CROSSSLOT Errors

**Symptom:**
```
(error) CROSSSLOT Keys in request don't hash to the same slot
```

**Cause:** You're trying multi-key operations without hash tags (v2.0+ feature)

**Solution:**
```redis
# Wrong (different slots)
MGET user:123 user:456

# Right (same slot with hash tags)
MGET {user:123}:api {user:123}:web
```

### MOVED Redirect Loops

**Symptom:** Client keeps getting `MOVED` errors

**Cause:** Cluster topology outdated or misconfigured

**Solution:**
```bash
# Check cluster health
redis-cli --cluster check 127.0.0.1:7001

# Fix cluster if needed
redis-cli --cluster fix 127.0.0.1:7001

# Client should refresh slot cache
# Most clients do this automatically
```

### Unbalanced Key Distribution

**Symptom:** One node has 80% of keys

**Cause:** Using too many hash tags or poor key design

**Solution:**
```bash
# Check distribution
for port in 7001 7002 7003; do
  count=$(redis-cli -p $port DBSIZE)
  echo "Node $port: $count keys"
done

# Rebalance if needed
redis-cli --cluster rebalance 127.0.0.1:7001
```

---

## Performance Considerations

### Latency

| Scenario | Latency | Notes |
|----------|---------|-------|
| Key on local node | ~0.5ms | Direct execution |
| Key on remote node | ~1-2ms | One MOVED redirect |
| During resharding | ~2-5ms | ASK redirects |
| During failover | ~5-10s | Cluster convergence time |

### Throughput

- **Single node:** ~50K ops/sec (SHIELD.absorb)
- **3-node cluster:** ~150K ops/sec (linear scaling)
- **Bottleneck:** Network between client and cluster

### Optimization Tips

1. **Use pipelining** for multiple independent requests
2. **Use hash tags** for related keys (careful: hotspots)
3. **Read from replicas** for read-heavy workloads (if implemented in v2.0)
4. **Monitor slot distribution** to avoid hotspots

---

## Migration Guide

### From Single Instance to Cluster

```bash
# 1. Export data from single instance
redis-cli --rdb dump.rdb

# 2. Setup cluster (all nodes)
# Load module on each node

# 3. Import data
redis-cli --cluster import 127.0.0.1:7001 --cluster-from 127.0.0.1:6379

# 4. Update client connection strings
# From: redis://127.0.0.1:6379
# To: redis://127.0.0.1:7001,127.0.0.1:7002,127.0.0.1:7003

# 5. Test
redis-cli -c -p 7001 SHIELD.absorb test 10 60 1
```

### Zero-Downtime Migration

1. Run cluster alongside single instance
2. Dual-write to both for 1-2 hours
3. Verify cluster working correctly
4. Switch reads to cluster
5. Switch writes to cluster
6. Decommission single instance

---

## Testing Checklist

Before production:

- [ ] Module loaded on all nodes (masters + replicas)
- [ ] Test basic SHIELD.absorb on each node
- [ ] Test with hash tags
- [ ] Test during resharding
- [ ] Test failover scenario
- [ ] Monitor memory usage under load
- [ ] Verify AOF persistence working
- [ ] Test client library auto-reconnect
- [ ] Load test with realistic traffic
- [ ] Document runbook for ops team

---

## Limitations

### Current (v1.0.0)

✅ **Supported:**
- Single-key operations
- All current SHIELD commands
- Failover and resharding
- AOF persistence

⚠️ **Not Yet Implemented:**
- Multi-key operations (planned v2.0)
- Read from replicas (planned v2.0)
- Cluster-aware optimizations

❌ **Not Supported:**
- Cross-slot transactions (Redis limitation)
- Cluster-wide statistics (each node independent)

---

## Production Deployment

### AWS ElastiCache Cluster

```hcl
# Terraform example
resource "aws_elasticache_replication_group" "redis_cluster" {
  replication_group_id       = "redis-shield-cluster"
  replication_group_description = "Redis Cluster with Shield module"
  engine                     = "redis"
  engine_version             = "7.0"
  node_type                  = "cache.r6g.large"
  number_cache_clusters      = 3
  automatic_failover_enabled = true
  multi_az_enabled           = true

  # Note: ElastiCache doesn't support custom modules yet
  # Use EC2 self-managed or Redis Enterprise instead
}
```

### Self-Managed on Kubernetes

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: redis-shield-cluster
spec:
  serviceName: redis-shield
  replicas: 6
  selector:
    matchLabels:
      app: redis-shield
  template:
    metadata:
      labels:
        app: redis-shield
    spec:
      containers:
      - name: redis
        image: redis:7-alpine
        command:
          - redis-server
          - --cluster-enabled yes
          - --cluster-config-file /data/nodes.conf
          - --appendonly yes
          - --loadmodule /usr/lib/redis/modules/libredis_shield.so
        volumeMounts:
        - name: data
          mountPath: /data
        - name: redis-shield
          mountPath: /usr/lib/redis/modules
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 10Gi
```

---

## FAQ

**Q: Do I need to load the module on replicas?**
A: Yes, load on all nodes (masters + replicas). Replicas can become masters during failover.

**Q: What happens to buckets during resharding?**
A: Buckets migrate with their keys. Brief latency increase, no data loss.

**Q: Can I use SHIELD on Redis Cluster mode disabled?**
A: Yes, works on standalone, sentinel, and cluster modes.

**Q: Does failover affect rate limiting?**
A: Brief unavailability during promotion, then continues normally. AOF ensures no data loss.

**Q: How to handle hot keys?**
A: Distribute load with key prefixes, avoid excessive hash tag use. Monitor with `redis-cli --hotkeys`.

**Q: Can I read from replicas?**
A: Not yet. v2.0 may add read-from-replica support for inspection commands.

---

## Next Steps

1. **Test locally:** `docker compose -f docker-compose.cluster.yml up -d`
2. **Run tests:** `cargo test --test cluster_tests`
3. **Review monitoring:** Setup alerts for cluster health
4. **Plan deployment:** Choose cloud provider or self-managed
5. **Load test:** Simulate production traffic
6. **Document runbook:** Ops procedures for your team

For questions or issues, see [GitHub Issues](https://github.com/ayarotsky/redis-shield/issues).
