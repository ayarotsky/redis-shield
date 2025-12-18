use crate::traffic_policy::TrafficPolicyExecutor;
use redis_module::{Context, RedisError, RedisString, RedisValue};

const MILLIS_IN_SEC: i64 = 1000;
const MIN_COUNT: i64 = 0;
const MIN_ACTIVE_TTL_MS: i64 = 1;
const INSUFFICIENT_CAPACITY: i64 = -1;

const ERR_CAPACITY_POSITIVE: &str = "ERR capacity must be positive";
const ERR_PERIOD_POSITIVE: &str = "ERR period must be positive";
const ERR_PERIOD_TOO_LARGE: &str = "ERR period value too large";
const ERR_HITS_POSITIVE: &str = "ERR tokens must be positive";
const ERR_INVALID_COUNTER: &str = "ERR invalid fixed window counter in Redis";

/// Fixed window rate limiter backed by Redis TTL.
///
/// Each window starts on the first request, allowing up to `capacity` hits.
/// Additional hits before the window expires are denied. We persist the count
/// in Redis and rely on TTL expiration to reset the window.
pub struct FixedWindow<'a> {
    pub key: &'a RedisString,
    pub capacity: i64,
    /// Window length in milliseconds.
    pub period: i64,
    /// Number of hits already recorded in the current window.
    pub count: i64,
    ctx: &'a Context,
    has_active_window: bool,
}

impl TrafficPolicyExecutor for FixedWindow<'_> {
    fn execute(&mut self, tokens: i64) -> Result<i64, RedisError> {
        self.consume(tokens)
    }
}

impl<'a> FixedWindow<'a> {
    #[inline]
    pub fn new(
        ctx: &'a Context,
        key: &'a RedisString,
        capacity: i64,
        period_sec: i64,
    ) -> Result<Self, RedisError> {
        if capacity <= 0 {
            return Err(RedisError::String(ERR_CAPACITY_POSITIVE.into()));
        }
        if period_sec <= 0 {
            return Err(RedisError::String(ERR_PERIOD_POSITIVE.into()));
        }

        let period = period_sec
            .checked_mul(MILLIS_IN_SEC)
            .ok_or(RedisError::String(ERR_PERIOD_TOO_LARGE.into()))?;

        let mut window = Self {
            ctx,
            key,
            capacity,
            period,
            count: MIN_COUNT,
            has_active_window: false,
        };
        window.fetch_count()?;
        Ok(window)
    }

    /// Consume `tokens` hits within the current fixed window.
    ///
    /// Returns remaining capacity if accepted, or `-1` if the window is full.
    #[inline]
    pub fn consume(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens <= 0 {
            return Err(RedisError::String(ERR_HITS_POSITIVE.into()));
        }

        if self.count.saturating_add(tokens) > self.capacity {
            return Ok(INSUFFICIENT_CAPACITY);
        }

        self.count = self.count.saturating_add(tokens).min(self.capacity);
        self.persist_count()?;

        Ok(self.capacity - self.count)
    }

    #[inline]
    fn fetch_count(&mut self) -> Result<(), RedisError> {
        self.has_active_window = matches!(
            self.ctx.call("PTTL", &[self.key])?,
            RedisValue::Integer(ttl) if ttl > MIN_ACTIVE_TTL_MS
        );

        if !self.has_active_window {
            self.count = MIN_COUNT;
            return Ok(());
        }

        self.count = match self.ctx.call("GET", &[self.key])? {
            RedisValue::SimpleString(value) => value
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_INVALID_COUNTER.into()))?
                .max(MIN_COUNT),
            _ => MIN_COUNT,
        };

        Ok(())
    }

    #[inline]
    fn persist_count(&mut self) -> Result<(), RedisError> {
        let mut count_buf = itoa::Buffer::new();
        let count_str = count_buf.format(self.count);

        if self.has_active_window {
            let keep_ttl = RedisString::create(None, "KEEPTTL");
            self.ctx.call(
                "SET",
                &[self.key, &RedisString::create(None, count_str), &keep_ttl],
            )?;
        } else {
            let mut period_buf = itoa::Buffer::new();
            let period_str = period_buf.format(self.period);
            self.ctx.call(
                "PSETEX",
                &[
                    self.key,
                    &RedisString::create(None, period_str),
                    &RedisString::create(None, count_str),
                ],
            )?;
            self.has_active_window = true;
        }

        Ok(())
    }
}
