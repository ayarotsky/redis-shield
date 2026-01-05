use crate::algorithm::{FixedWindow, LeakyBucket, SlidingWindow, TokenBucket};
use arrayvec::ArrayString;
use redis_module::{Context, RedisError, RedisString};
use std::fmt::Write;

pub const TRAFFIC_POLICY_KEY_PREFIX: &str = "tp";
const STACK_KEY_CAPACITY: usize = 128;

/// Storage used to hold the formatted Redis key.
pub(crate) enum KeyBuffer {
    Stack(ArrayString<STACK_KEY_CAPACITY>),
    Heap(String),
}

impl KeyBuffer {
    #[inline]
    /// Returns the buffered key as a string slice, regardless of backing storage.
    pub(crate) fn as_str(&self) -> &str {
        match self {
            KeyBuffer::Stack(buf) => buf.as_str(),
            KeyBuffer::Heap(s) => s.as_str(),
        }
    }
}

/// Common interface implemented by all rate-limit algorithms.
pub trait TrafficPolicyExecutor {
    /// Executes the policy consuming `tokens`, returning remaining capacity or `-1` on denial.
    fn execute(&mut self, tokens: i64) -> Result<i64, RedisError>;
}

pub enum PolicyConfig {
    TokenBucket { capacity: i64, period: i64 },
    LeakyBucket { capacity: i64, period: i64 },
    FixedWindow { capacity: i64, period: i64 },
    SlidingWindow { capacity: i64, period: i64 },
}

impl PolicyConfig {
    #[inline]
    pub(crate) fn suffix(&self) -> &'static str {
        match self {
            PolicyConfig::TokenBucket { .. } => "tb",
            PolicyConfig::LeakyBucket { .. } => "lb",
            PolicyConfig::FixedWindow { .. } => "fw",
            PolicyConfig::SlidingWindow { .. } => "sw",
        }
    }
}

/// Instantiates a rate-limiting executor backed by the supplied Redis context and policy config.
pub fn create_executor<'a>(
    cfg: PolicyConfig,
    ctx: &'a Context,
    key: &str,
) -> Result<Box<dyn TrafficPolicyExecutor + 'a>, RedisError> {
    let key_buf = build_key(key, cfg.suffix());
    let internal_key = RedisString::create(std::ptr::NonNull::new(ctx.ctx), key_buf.as_str());
    match cfg {
        PolicyConfig::TokenBucket { capacity, period } => Ok(Box::new(TokenBucket::new(
            ctx,
            internal_key,
            capacity,
            period,
        )?)),
        PolicyConfig::LeakyBucket { capacity, period } => Ok(Box::new(LeakyBucket::new(
            ctx,
            internal_key,
            capacity,
            period,
        )?)),
        PolicyConfig::FixedWindow { capacity, period } => Ok(Box::new(FixedWindow::new(
            ctx,
            internal_key,
            capacity,
            period,
        )?)),
        PolicyConfig::SlidingWindow { capacity, period } => Ok(Box::new(SlidingWindow::new(
            ctx,
            internal_key,
            capacity,
            period,
        )?)),
    }
}

/// Builds the internal Redis key, preferring stack storage and falling back to heap allocation.
pub(crate) fn build_key(external_key: &str, suffix: &str) -> KeyBuffer {
    let mut key_buf = ArrayString::<STACK_KEY_CAPACITY>::new();
    if write!(
        &mut key_buf,
        "{}:{}:{}",
        TRAFFIC_POLICY_KEY_PREFIX, suffix, external_key
    )
    .is_ok()
    {
        KeyBuffer::Stack(key_buf)
    } else {
        // Fallback to heap allocation if the user-provided key exceeds our stack buffer.
        KeyBuffer::Heap(format!(
            "{}:{}:{}",
            TRAFFIC_POLICY_KEY_PREFIX, suffix, external_key
        ))
    }
}
