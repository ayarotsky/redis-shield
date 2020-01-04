mod bucket;

#[macro_use] extern crate redis_module;
use redis_module::{Context, RedisError, RedisResult, parse_integer};
use bucket::Bucket;

fn redis_command(ctx: &Context, args: Vec<String>) -> RedisResult {
    if args.len() != 4 {
        return Err(RedisError::WrongArity);
    }

    ctx.auto_memory();

    let capacity = parse_integer(&args[2])?;
    let tokens = parse_integer(&args[3])?;
    let mut bucket = Bucket::new(ctx, &args[1], capacity)?;
    let remaining_tokens = bucket.pour(tokens)?;

    Ok(remaining_tokens.into())
}

redis_module! {
    name: "SHIELD",
    version: 1,
    data_types: [],
    commands: [
        ["SHIELD.absorb", redis_command, ""],
    ],
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
