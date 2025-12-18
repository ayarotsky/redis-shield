use crate::algorithm::{FixedWindow, LeakyBucket, SlidingWindow, TokenBucket};
use redis_module::{Context, RedisError, RedisString};

const TRAFFIC_POLICY_KEY_PREFIX: &str = "tp";

pub trait TrafficPolicyExecutor {
    fn execute(&mut self, tokens: i64) -> Result<i64, RedisError>;
}

trait TrafficPolicySuffix {
    fn suffix(&self) -> &'static str;
}

pub enum PolicyConfig {
    TokenBucket { capacity: i64, period: i64 },
    LeakyBucket { capacity: i64, period: i64 },
    FixedWindow { capacity: i64, period: i64 },
    SlidingWindow { capacity: i64, period: i64 },
}

impl TrafficPolicySuffix for PolicyConfig {
    fn suffix(&self) -> &'static str {
        match self {
            PolicyConfig::TokenBucket { .. } => "tb",
            PolicyConfig::LeakyBucket { .. } => "lb",
            PolicyConfig::FixedWindow { .. } => "fw",
            PolicyConfig::SlidingWindow { .. } => "sw",
        }
    }
}

pub fn create_executor<'a>(
    cfg: PolicyConfig,
    ctx: &'a Context,
    key: RedisString,
) -> Result<Box<dyn TrafficPolicyExecutor + 'a>, RedisError> {
    let owned_key = Box::new(RedisString::create(
        std::ptr::NonNull::new(ctx.ctx),
        build_key(key.to_string_lossy().as_str(), cfg.suffix()),
    ));
    let key_ref: &'a RedisString = Box::leak(owned_key);
    match cfg {
        PolicyConfig::TokenBucket { capacity, period } => {
            Ok(Box::new(TokenBucket::new(ctx, key_ref, capacity, period)?))
        }
        PolicyConfig::LeakyBucket { capacity, period } => {
            Ok(Box::new(LeakyBucket::new(ctx, key_ref, capacity, period)?))
        }
        PolicyConfig::FixedWindow { capacity, period } => {
            Ok(Box::new(FixedWindow::new(ctx, key_ref, capacity, period)?))
        }
        PolicyConfig::SlidingWindow { capacity, period } => Ok(Box::new(SlidingWindow::new(
            ctx, key_ref, capacity, period,
        )?)),
    }
}

fn build_key(external_key: &str, suffix: &str) -> String {
    format!("{}:{}:{}", TRAFFIC_POLICY_KEY_PREFIX, suffix, external_key)
}
