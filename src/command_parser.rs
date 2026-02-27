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
pub struct CommandInvocation<'a> {
    pub key: &'a str,
    pub cfg: PolicyConfig,
    pub tokens: i64,
}

#[inline]
fn get(args: &[RedisString], idx: usize) -> Result<&str, RedisError> {
    args.get(idx).ok_or(RedisError::WrongArity)?.try_as_str()
}

/// Validates and parses the raw Redis arguments into a [`CommandInvocation`].
///
/// This ensures arity, parses algorithm selections, and normalizes optional tokens.
pub fn parse_command_args(args: &[RedisString]) -> Result<CommandInvocation<'_>, RedisError> {
    parse_command_args_inner(args.len(), |idx| get(args, idx))
}

#[inline]
fn parse_command_args_inner<'a, F>(
    len: usize,
    mut get: F,
) -> Result<CommandInvocation<'a>, RedisError>
where
    F: FnMut(usize) -> Result<&'a str, RedisError>,
{
    if !matches!(len, MIN_ARGS_LEN..=MAX_ARGS_LEN) {
        return Err(RedisError::WrongArity);
    }

    let (algorithm, tokens) = match len {
        MIN_ARGS_LEN => (DEFAULT_ALGORITHM, DEFAULT_TOKENS),

        5 => {
            let candidate = get(ARG_TOKENS_INDEX)?;
            if candidate.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG) {
                return Err(RedisError::Str(ERR_ALGORITHM_VALUE_MISSING));
            }
            (
                DEFAULT_ALGORITHM,
                parse_positive_integer(candidate, ERR_TOKENS_POSITIVE)?,
            )
        }

        6 => {
            let candidate = get(ARG_TOKENS_INDEX)?;
            if candidate.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG) {
                let algorithm = get(ARG_TOKENS_INDEX + 1)
                    .map_err(|_| RedisError::Str(ERR_ALGORITHM_VALUE_MISSING))?;
                (algorithm, DEFAULT_TOKENS)
            } else {
                let next = get(ARG_TOKENS_INDEX + 1)?;
                if next.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG) {
                    return Err(RedisError::Str(ERR_ALGORITHM_VALUE_MISSING));
                }
                return Err(RedisError::WrongArity);
            }
        }

        7 => {
            let flag = get(ARG_TOKENS_INDEX + 1)?;
            if !flag.eq_ignore_ascii_case(ARG_ALGORITHM_FLAG) {
                return Err(RedisError::WrongArity);
            }
            let algorithm = get(ARG_TOKENS_INDEX + 2)
                .map_err(|_| RedisError::Str(ERR_ALGORITHM_VALUE_MISSING))?;
            let tokens_str = get(ARG_TOKENS_INDEX)?;
            (
                algorithm,
                parse_positive_integer(tokens_str, ERR_TOKENS_POSITIVE)?,
            )
        }

        _ => return Err(RedisError::WrongArity),
    };

    let capacity = parse_positive_integer(get(ARG_CAPACITY_INDEX)?, ERR_CAPACITY_POSITIVE)?;
    let period = parse_positive_integer(get(ARG_PERIOD_INDEX)?, ERR_PERIOD_POSITIVE)?;

    let cfg = match algorithm {
        "token_bucket" => PolicyConfig::TokenBucket { capacity, period },
        "leaky_bucket" => PolicyConfig::LeakyBucket { capacity, period },
        "fixed_window" => PolicyConfig::FixedWindow { capacity, period },
        "sliding_window" => PolicyConfig::SlidingWindow { capacity, period },
        _ => return Err(RedisError::Str(ERR_UNKNOWN_ALGORITHM)),
    };

    let key = get(ARG_KEY_INDEX)?;
    Ok(CommandInvocation { key, cfg, tokens })
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
fn parse_positive_integer(value: &str, err_msg: &'static str) -> Result<i64, RedisError> {
    match value.parse::<i64>() {
        Ok(arg) if arg > 0 => Ok(arg),
        _ => Err(RedisError::Str(err_msg)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_command_args_test<'a>(
        args: &'a [&'a str],
    ) -> Result<CommandInvocation<'a>, RedisError> {
        parse_command_args_inner(args.len(), |idx| {
            args.get(idx).copied().ok_or(RedisError::WrongArity)
        })
    }

    #[test]
    fn parse_base_args_defaults_tokens_and_algorithm() {
        let args = ["SHIELD.absorb", "user1", "10", "60"];
        let invocation = parse_command_args_test(&args).expect("parse base args");

        assert_eq!(invocation.tokens, DEFAULT_TOKENS);
        match invocation.cfg {
            PolicyConfig::TokenBucket { capacity, period } => {
                assert_eq!(capacity, 10);
                assert_eq!(period, 60);
            }
            _ => panic!("expected token_bucket config"),
        }
    }

    #[test]
    fn parse_args_with_tokens() {
        let args = ["SHIELD.absorb", "user1", "10", "60", "5"];
        let invocation = parse_command_args_test(&args).expect("parse tokens");

        assert_eq!(invocation.tokens, 5);
        match invocation.cfg {
            PolicyConfig::TokenBucket { capacity, period } => {
                assert_eq!(capacity, 10);
                assert_eq!(period, 60);
            }
            _ => panic!("expected token_bucket config"),
        }
    }

    #[test]
    fn parse_args_with_algorithm_only() {
        let args = [
            "SHIELD.absorb",
            "user1",
            "10",
            "60",
            "ALGORITHM",
            "fixed_window",
        ];
        let invocation = parse_command_args_test(&args).expect("parse algorithm only");

        assert_eq!(invocation.tokens, DEFAULT_TOKENS);
        match invocation.cfg {
            PolicyConfig::FixedWindow { capacity, period } => {
                assert_eq!(capacity, 10);
                assert_eq!(period, 60);
            }
            _ => panic!("expected fixed_window config"),
        }
    }

    #[test]
    fn parse_args_with_tokens_and_algorithm() {
        let args = [
            "SHIELD.absorb",
            "user1",
            "10",
            "60",
            "5",
            "ALGORITHM",
            "sliding_window",
        ];
        let invocation = parse_command_args_test(&args).expect("parse tokens + algorithm");

        assert_eq!(invocation.tokens, 5);
        match invocation.cfg {
            PolicyConfig::SlidingWindow { capacity, period } => {
                assert_eq!(capacity, 10);
                assert_eq!(period, 60);
            }
            _ => panic!("expected sliding_window config"),
        }
    }

    #[test]
    fn parse_args_with_algorithm_missing_value() {
        let args = ["SHIELD.absorb", "user1", "10", "60", "ALGORITHM"];
        match parse_command_args_test(&args) {
            Err(RedisError::Str(msg)) => assert_eq!(msg, ERR_ALGORITHM_VALUE_MISSING),
            _ => panic!("expected algorithm value missing error"),
        }
    }

    #[test]
    fn parse_args_rejects_algorithm_before_tokens() {
        let args = [
            "SHIELD.absorb",
            "user1",
            "10",
            "60",
            "ALGORITHM",
            "fixed_window",
            "5",
        ];
        match parse_command_args_test(&args) {
            Err(RedisError::WrongArity) => {}
            _ => panic!("expected wrong arity for invalid argument order"),
        }
    }
}
