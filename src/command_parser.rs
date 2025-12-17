use crate::traffic_policy::PolicyConfig;
use redis_module::{RedisError, RedisString};

// Command argument constraints
const MIN_ARGS_LEN: usize = 4;
const MAX_ARGS_LEN: usize = 7;
// Argument indices
const ARG_KEY_INDEX: usize = 1;
const ARG_CAPACITY_INDEX: usize = 2;
const ARG_PERIOD_INDEX: usize = 3;
const ARG_TOKENS_INDEX: usize = 4;
// Default values
pub const DEFAULT_TOKENS: i64 = 1;
const ARG_ALGORITHM_FLAG: &str = "ALGORITHM";

pub struct CommandInvocation {
    pub key: RedisString,
    pub cfg: PolicyConfig,
    pub tokens: i64,
}

pub fn parse_command_args(args: &[RedisString]) -> Result<CommandInvocation, RedisError> {
    // Validate argument count
    if !matches!(args.len(), MIN_ARGS_LEN..=MAX_ARGS_LEN) {
        return Err(RedisError::WrongArity);
    }
    // Parse algorithm argument, default to "token_bucket" if not provided
    let algorithm = parse_algorithm_arg(args)?.unwrap_or("token_bucket".to_owned());

    // Create algorithm configuration
    let config = create_algorithm_config(algorithm, args)?;
    let tokens = match args.len() {
        MAX_ARGS_LEN => parse_positive_integer("tokens", &args[ARG_TOKENS_INDEX])?,
        _ => DEFAULT_TOKENS,
    };
    let key = args[ARG_KEY_INDEX].clone();
    Ok(CommandInvocation {
        key,
        cfg: config,
        tokens,
    })
}

#[inline]
fn parse_algorithm_arg(args: &[RedisString]) -> Result<Option<String>, RedisError> {
    let iter = args.iter().enumerate();

    for (i, arg) in iter {
        let key = arg.try_as_str()?;

        if key.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG) {
            let value = args
                .get(i + 1)
                .ok_or(RedisError::Str("ERR algorithm value missing"))?;

            return Ok(Some(value.try_as_str()?.to_owned()));
        }
    }

    Ok(None) // algorithm not provided
}

#[inline]
fn create_algorithm_config(
    algorithm: String,
    args: &[RedisString],
) -> Result<PolicyConfig, RedisError> {
    // Parse and validate arguments
    let capacity = parse_positive_integer("capacity", &args[ARG_CAPACITY_INDEX])?;
    let period = parse_positive_integer("period/window", &args[ARG_PERIOD_INDEX])?;
    match algorithm.as_str() {
        "token_bucket" => Ok(PolicyConfig::TokenBucket{capacity, period}),
        "leaky_bucket" => Ok(PolicyConfig::LeakyBucket{capacity, period}),
        // "fixed_window" => Ok(PolicyConfig::FixedWindow{capacity, period}),
        // "sliding_window" => Ok(PolicyConfig::SlidingWindow{capacity, period}),
        _ => Err(RedisError::String(format!("ERR unknown algorithm {}, supported are [token_bucket, leaky_bucket, fixed_window, sliding_window]", algorithm))),
    }
}

/// Parses a RedisString argument as a positive integer.
///
/// # Arguments
/// * `name` - The name of the parameter for error messages
/// * `value` - The RedisString value to parse
///
/// # Returns
/// * `Ok(i64)` - The parsed positive integer
/// * `Err(RedisError)` - If the value is not a positive integer
///
/// # Errors
/// Returns a RedisError with a descriptive message if:
/// - The value cannot be parsed as an integer
/// - The parsed integer is not positive (≤ 0)
#[inline]
fn parse_positive_integer(name: &str, value: &RedisString) -> Result<i64, RedisError> {
    match value.parse_integer() {
        Ok(arg) if arg > 0 => Ok(arg),
        _ => Err(RedisError::String(format!("ERR {} must be positive", name))),
    }
}
