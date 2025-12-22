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
pub const ARG_ALGORITHM_FLAG: &str = "ALGORITHM";
const DEFAULT_ALGORITHM: &str = "token_bucket";

// Error messages
const ERR_ALGORITHM_VALUE_MISSING: &str = "ERR algorithm value missing";
const ERR_UNKNOWN_ALGORITHM: &str =
    "ERR unknown algorithm, supported are [token_bucket, leaky_bucket, fixed_window, sliding_window]";
const ERR_CAPACITY_POSITIVE: &str = "ERR capacity must be positive";
const ERR_PERIOD_POSITIVE: &str = "ERR period/window must be positive";
const ERR_TOKENS_POSITIVE: &str = "ERR tokens must be positive";

/// Parsed representation of a `SHIELD.absorb` invocation.
pub struct CommandInvocation {
    pub key: RedisString,
    pub cfg: PolicyConfig,
    pub tokens: i64,
}

/// Validates and parses the raw Redis arguments into a [`CommandInvocation`].
///
/// This ensures arity, parses algorithm selections, and normalizes optional tokens.
pub fn parse_command_args(args: &[RedisString]) -> Result<CommandInvocation, RedisError> {
    // Validate argument count
    if !matches!(args.len(), MIN_ARGS_LEN..=MAX_ARGS_LEN) {
        return Err(RedisError::WrongArity);
    }
    // Parse algorithm argument, default to "token_bucket" if not provided
    let algorithm = parse_algorithm_arg(args)?.unwrap_or(DEFAULT_ALGORITHM);

    // Create algorithm configuration
    let config = create_algorithm_config(algorithm, args)?;
    // Parse optional tokens argument
    let tokens = if args.len() > ARG_TOKENS_INDEX {
        let potential_tokens = &args[ARG_TOKENS_INDEX];
        if potential_tokens
            .try_as_str()
            .map(|s| s.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG))
            .unwrap_or(false)
        {
            DEFAULT_TOKENS
        } else {
            parse_positive_integer(potential_tokens, ERR_TOKENS_POSITIVE)?
        }
    } else {
        DEFAULT_TOKENS
    };
    let key = args[ARG_KEY_INDEX].clone();
    Ok(CommandInvocation {
        key,
        cfg: config,
        tokens,
    })
}

/// Scans the optional section of the argument list for an `ALGORITHM <name>` pair.
///
/// Returns `Ok(None)` when no algorithm override is provided.
#[inline]
fn parse_algorithm_arg(args: &[RedisString]) -> Result<Option<&str>, RedisError> {
    if args.len() <= ARG_PERIOD_INDEX + 1 {
        // Not enough arguments to contain an ALGORITHM flag and value.
        return Ok(None);
    }

    let mut idx = ARG_PERIOD_INDEX + 1;
    while idx < args.len() {
        let key = args[idx].try_as_str()?;
        if key.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG) {
            let value = args
                .get(idx + 1)
                .ok_or(RedisError::Str(ERR_ALGORITHM_VALUE_MISSING))?;
            return Ok(Some(value.try_as_str()?));
        }
        idx += 1;
    }

    Ok(None) // algorithm not provided
}

/// Builds the [`PolicyConfig`] requested by the user, validating shared parameters.
#[inline]
fn create_algorithm_config(
    algorithm: &str,
    args: &[RedisString],
) -> Result<PolicyConfig, RedisError> {
    // Parse and validate arguments
    let capacity = parse_positive_integer(&args[ARG_CAPACITY_INDEX], ERR_CAPACITY_POSITIVE)?;
    let period = parse_positive_integer(&args[ARG_PERIOD_INDEX], ERR_PERIOD_POSITIVE)?;
    match algorithm {
        "token_bucket" => Ok(PolicyConfig::TokenBucket { capacity, period }),
        "leaky_bucket" => Ok(PolicyConfig::LeakyBucket { capacity, period }),
        "fixed_window" => Ok(PolicyConfig::FixedWindow { capacity, period }),
        "sliding_window" => Ok(PolicyConfig::SlidingWindow { capacity, period }),
        _ => Err(RedisError::Str(ERR_UNKNOWN_ALGORITHM)),
    }
}

/// Parses a RedisString argument as a positive integer.
///
/// # Arguments
/// * `value` - The RedisString value to parse
/// * `err_msg` - Static error to return when validation fails
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
fn parse_positive_integer(value: &RedisString, err_msg: &'static str) -> Result<i64, RedisError> {
    match value.parse_integer() {
        Ok(arg) if arg > 0 => Ok(arg),
        _ => Err(RedisError::Str(err_msg)),
    }
}
