use crate::traffic_policy::TrafficPolicyExecutor;
use redis_module::{Context, RedisError, RedisString, RedisValue};

const MILLIS_IN_SEC: i64 = 1000;
const MICROS_IN_MILLI: i64 = 1000;
const MIN_COUNT: i64 = 0;
const INSUFFICIENT_CAPACITY: i64 = -1;
const STATE_BUFFER_LEN: usize = 96;

const ERR_CAPACITY_POSITIVE: &str = "ERR capacity must be positive";
const ERR_PERIOD_POSITIVE: &str = "ERR period must be positive";
const ERR_PERIOD_TOO_LARGE: &str = "ERR period value too large";
const ERR_TOKENS_POSITIVE: &str = "ERR tokens must be positive";
const ERR_TIME_UNAVAILABLE: &str = "ERR unable to fetch Redis time";

/// Sliding window limiter backed by Redis.
///
/// Keeps counters for the current discrete window and the immediately
/// preceding window. The effective usage interpolates between both
/// windows based on the elapsed time to approximate a true sliding window.
pub struct SlidingWindow<'a> {
    pub key: &'a RedisString,
    pub capacity: i64,
    /// Window length in milliseconds.
    pub period: i64,
    ctx: &'a Context,
    current_start: i64,
    current_count: i64,
    previous_count: i64,
}

impl TrafficPolicyExecutor for SlidingWindow<'_> {
    fn execute(&mut self, tokens: i64) -> Result<i64, RedisError> {
        self.consume(tokens)
    }
}

impl<'a> SlidingWindow<'a> {
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

        let now_ms = Self::current_time_millis(ctx)?;
        let mut limiter = Self {
            ctx,
            key,
            capacity,
            period,
            current_start: now_ms,
            current_count: MIN_COUNT,
            previous_count: MIN_COUNT,
        };
        limiter.load_state(now_ms)?;
        Ok(limiter)
    }

    #[inline]
    pub fn consume(&mut self, tokens: i64) -> Result<i64, RedisError> {
        if tokens <= 0 {
            return Err(RedisError::String(ERR_TOKENS_POSITIVE.into()));
        }

        let now_ms = Self::current_time_millis(self.ctx)?;
        let elapsed = self.align_to_now(now_ms);
        let usage = self.effective_usage(elapsed);

        if usage.saturating_add(tokens) > self.capacity {
            return Ok(INSUFFICIENT_CAPACITY);
        }

        self.current_count = self.current_count.saturating_add(tokens).min(self.capacity);
        self.persist_state()?;

        let remaining = self.capacity - usage - tokens;
        Ok(remaining.max(MIN_COUNT))
    }

    #[inline]
    fn load_state(&mut self, now_ms: i64) -> Result<(), RedisError> {
        match self.ctx.call("GET", &[self.key])? {
            RedisValue::SimpleString(payload) | RedisValue::BulkString(payload) => {
                self.apply_state(&payload, now_ms);
            }
            RedisValue::BulkRedisString(value) => {
                let payload = value.try_as_str()?;
                self.apply_state(payload, now_ms);
            }
            RedisValue::Null => {}
            _ => {
                // Different Redis type stored under the key – treat as empty window.
                self.current_start = now_ms;
                self.current_count = MIN_COUNT;
                self.previous_count = MIN_COUNT;
            }
        }
        Ok(())
    }

    #[inline]
    fn apply_state(&mut self, payload: &str, now_ms: i64) {
        if let Some((start, current, previous)) = Self::decode_state(payload) {
            self.current_start = start.max(MIN_COUNT).min(now_ms);
            self.current_count = current.max(MIN_COUNT).min(self.capacity);
            self.previous_count = previous.max(MIN_COUNT).min(self.capacity);
        } else {
            self.current_start = now_ms;
            self.current_count = MIN_COUNT;
            self.previous_count = MIN_COUNT;
        }
        let _ = self.align_to_now(now_ms);
    }

    #[inline]
    fn align_to_now(&mut self, now_ms: i64) -> i64 {
        if self.current_start > now_ms || self.current_start < 0 {
            self.current_start = now_ms;
        }

        let mut elapsed = now_ms.saturating_sub(self.current_start);
        if elapsed >= self.period {
            let windows_passed = elapsed / self.period;
            if windows_passed == 1 {
                self.previous_count = self.current_count;
            } else {
                self.previous_count = MIN_COUNT;
            }
            self.current_count = MIN_COUNT;

            let advance = windows_passed
                .checked_mul(self.period)
                .unwrap_or(self.period);
            self.current_start = self.current_start.saturating_add(advance);
            if self.current_start > now_ms {
                self.current_start = now_ms;
            }
            elapsed = now_ms.saturating_sub(self.current_start);
        }
        elapsed
    }

    #[inline]
    fn effective_usage(&self, elapsed: i64) -> i64 {
        let remaining = self.period.saturating_sub(elapsed);
        let weighted_previous = if self.period == 0 {
            MIN_COUNT
        } else {
            ((self.previous_count as i128) * (remaining as i128) / self.period as i128) as i64
        };
        self.current_count
            .saturating_add(weighted_previous)
            .min(self.capacity)
    }

    #[inline]
    fn persist_state(&self) -> Result<(), RedisError> {
        let mut state_buf = [0u8; STATE_BUFFER_LEN];
        let state_str = self.encode_state(&mut state_buf);
        let ttl = self.period.saturating_mul(2).max(self.period);

        let mut ttl_buf = itoa::Buffer::new();
        let ttl_str = ttl_buf.format(ttl);

        self.ctx.call(
            "PSETEX",
            &[
                self.key,
                &RedisString::create(None, ttl_str),
                &RedisString::create(None, state_str),
            ],
        )?;
        Ok(())
    }

    #[inline]
    fn encode_state<'b>(&self, buf: &'b mut [u8; STATE_BUFFER_LEN]) -> &'b str {
        let mut cursor = 0;
        cursor += Self::copy_number(buf, cursor, self.current_start);
        buf[cursor] = b':';
        cursor += 1;
        cursor += Self::copy_number(buf, cursor, self.current_count);
        buf[cursor] = b':';
        cursor += 1;
        cursor += Self::copy_number(buf, cursor, self.previous_count);
        std::str::from_utf8(&buf[..cursor]).unwrap()
    }

    #[inline]
    fn copy_number(buf: &mut [u8; STATE_BUFFER_LEN], offset: usize, value: i64) -> usize {
        let mut num_buf = itoa::Buffer::new();
        let digits = num_buf.format(value);
        let len = digits.len();
        buf[offset..offset + len].copy_from_slice(digits.as_bytes());
        len
    }

    #[inline]
    fn decode_state(payload: &str) -> Option<(i64, i64, i64)> {
        let mut parts = payload.splitn(3, ':');
        let start = parts.next()?.parse::<i64>().ok()?;
        let current = parts.next()?.parse::<i64>().ok()?;
        let previous = parts.next()?.parse::<i64>().ok()?;
        Some((start, current, previous))
    }

    #[inline]
    fn current_time_millis(ctx: &Context) -> Result<i64, RedisError> {
        let empty_args: [&RedisString; 0] = [];
        match ctx.call("TIME", &empty_args)? {
            RedisValue::Array(values) if values.len() == 2 => {
                let seconds = Self::parse_i64(&values[0])?;
                let micros = Self::parse_i64(&values[1])?;
                let millis_from_secs = seconds
                    .checked_mul(MILLIS_IN_SEC)
                    .ok_or(RedisError::String(ERR_TIME_UNAVAILABLE.into()))?;
                let millis_from_micros = micros / MICROS_IN_MILLI;
                millis_from_secs
                    .checked_add(millis_from_micros)
                    .ok_or(RedisError::String(ERR_TIME_UNAVAILABLE.into()))
            }
            _ => Err(RedisError::String(ERR_TIME_UNAVAILABLE.into())),
        }
    }

    #[inline]
    fn parse_i64(value: &RedisValue) -> Result<i64, RedisError> {
        match value {
            RedisValue::Integer(v) => Ok(*v),
            RedisValue::SimpleString(s) | RedisValue::BulkString(s) => s
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_TIME_UNAVAILABLE.into())),
            RedisValue::SimpleStringStatic(s) => s
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_TIME_UNAVAILABLE.into())),
            RedisValue::BulkRedisString(s) => s
                .try_as_str()?
                .parse::<i64>()
                .map_err(|_| RedisError::String(ERR_TIME_UNAVAILABLE.into())),
            _ => Err(RedisError::String(ERR_TIME_UNAVAILABLE.into())),
        }
    }
}
