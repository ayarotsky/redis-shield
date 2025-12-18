use crate::traffic_policy::TrafficPolicyExecutor;
use redis_module::{Context, RedisError, RedisString, RedisValue};

const MILLIS_IN_SEC: i64 = 1000;
const MIN_TTL: i64 = 0;
const MIN_LEVEL: i64 = 0;
const OVERFLOW: i64 = -1;

const ERR_CAPACITY_POSITIVE: &str = "ERR capacity must be positive";
const ERR_PERIOD_POSITIVE: &str = "ERR period must be positive";
const ERR_PERIOD_TOO_LARGE: &str = "ERR period value too large";
const ERR_BURST_POSITIVE: &str = "ERR burst must be positive";
const ERR_INVALID_LEVEL: &str = "ERR invalid bucket level in Redis";

/// Leaky Bucket rate limiter backed by Redis TTL.
///
/// The bucket has a maximum `capacity` and leaks at a constant rate
/// defined by `capacity` per `period` (i.e., capacity tokens every period seconds).
/// Incoming requests add water (units) to the bucket. If adding would exceed
/// `capacity`, the request is denied; otherwise accepted and persisted.
///
/// We model the leak using TTL like in token bucket: shorter remaining TTL
/// -> more elapsed time -> more leaked units since last update.
pub struct LeakyBucket<'a> {
    pub key: RedisString,
    pub capacity: i64,
    /// Period in milliseconds (derived from seconds input)
    pub period: i64,
    /// Current water level (units in bucket)
    pub level: i64,
    ctx: &'a Context,
}

impl TrafficPolicyExecutor for LeakyBucket<'_> {
    fn execute(&mut self, tokens: i64) -> Result<i64, RedisError> {
        self.add(tokens)
    }
}

impl<'a> LeakyBucket<'a> {
    /// Instantiate a new leaky bucket or load existing state.
    ///
    /// `capacity` is the maximum allowed level.
    /// `period_sec` defines the leak rate: capacity per period.
    #[inline]
    pub fn new(
        ctx: &'a Context,
        key: RedisString,
        capacity: i64,
        period_sec: i64,
    ) -> Result<Self, RedisError> {
        if capacity <= 0 {
            return Err(RedisError::String(ERR_CAPACITY_POSITIVE.into()));
        }
        if period_sec <= 0 {
            return Err(RedisError::String(ERR_PERIOD_POSITIVE.into()));
        }

        let period_ms = period_sec
            .checked_mul(MILLIS_IN_SEC)
            .ok_or(RedisError::String(ERR_PERIOD_TOO_LARGE.into()))?;

        let mut bucket = Self {
            ctx,
            key,
            capacity,
            period: period_ms,
            level: MIN_LEVEL,
        };
        bucket.fetch_level()?;
        Ok(bucket)
    }

    /// Try to add `burst` units to the bucket.
    /// Returns remaining headroom (capacity - level) if accepted,
    /// or `-1` if it would overflow.
    #[inline]
    pub fn add(&mut self, burst: i64) -> Result<i64, RedisError> {
        if burst <= 0 {
            return Err(RedisError::String(ERR_BURST_POSITIVE.into()));
        }

        // If adding would exceed capacity, deny.
        if self.level.saturating_add(burst) > self.capacity {
            return Ok(OVERFLOW);
        }

        self.level = self.level.saturating_add(burst).min(self.capacity);

        let mut period_buf = itoa::Buffer::new();
        let mut level_buf = itoa::Buffer::new();
        let period_str = period_buf.format(self.period);
        let level_str = level_buf.format(self.level);

        // Persist new level with fresh TTL for the period window.
        self.ctx.call(
            "PSETEX",
            &[
                &self.key,
                &RedisString::create(None, period_str),
                &RedisString::create(None, level_str),
            ],
        )?;

        let headroom = self.capacity - self.level;
        Ok(headroom)
    }

    /// Fetch current level and apply leak based on elapsed time.
    #[inline]
    fn fetch_level(&mut self) -> Result<(), RedisError> {
        // Clamp TTL into [0, period]
        let current_ttl = match self.ctx.call("PTTL", &[&self.key])? {
            RedisValue::Integer(ttl) => ttl.clamp(MIN_TTL, self.period),
            _ => MIN_TTL,
        };

        // Elapsed time since last persist
        let elapsed = self.period - current_ttl;
        // Leak amount: capacity * elapsed / period
        let leaked = ((elapsed as i128 * self.capacity as i128) / self.period as i128) as i64;

        // Load stored level (if any)
        let stored_level = match self.ctx.call("GET", &[&self.key])? {
            RedisValue::SimpleString(s) | RedisValue::BulkString(s) => s
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_INVALID_LEVEL.into()))?
                .max(MIN_LEVEL),
            RedisValue::BulkRedisString(s) => s
                .try_as_str()?
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_INVALID_LEVEL.into()))?
                .max(MIN_LEVEL),
            _ => MIN_LEVEL,
        };

        // Apply leak: level decreases by leaked, floored at 0
        let new_level = stored_level.saturating_sub(leaked);
        self.level = new_level.min(self.capacity);
        Ok(())
    }
}
