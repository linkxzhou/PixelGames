use std::collections::BTreeMap;

use pxl_core::{PxlError, PxlResult};

use super::catalog::allowed_args;

pub fn parse_macro_call(spec: &str) -> PxlResult<(String, BTreeMap<String, String>)> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(PxlError::usage("missing macro").with_hint("example: tread_cycle(steps=4)"));
    }
    let (name, rest) = if let Some(open) = spec.find('(') {
        if !spec.ends_with(')') {
            return Err(PxlError::schema(format!("macro '{spec}' is missing ')'"))
                .with_hint("write name(arg=value, arg=value)"));
        }
        (&spec[..open], &spec[open + 1..spec.len() - 1])
    } else {
        (spec, "")
    };
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(PxlError::schema(format!("bad macro name '{name}'")));
    }
    let mut args = BTreeMap::new();
    if !rest.trim().is_empty() {
        for part in rest.split(',') {
            let part = part.trim();
            let Some((k, v)) = part.split_once('=') else {
                return Err(
                    PxlError::schema(format!("macro arg '{part}' needs key=value"))
                        .with_hint("example: steps=4, region=auto_tread"),
                );
            };
            args.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Ok((name.to_string(), args))
}

pub(super) fn check_args(name: &str, args: &BTreeMap<String, String>) -> PxlResult<()> {
    let allowed = allowed_args(name);
    for key in args.keys() {
        if !allowed.contains(&key.as_str()) {
            let hint = if allowed.is_empty() {
                format!("{name} takes no arguments")
            } else {
                format!("{name} takes: {}", allowed.join(", "))
            };
            return Err(
                PxlError::schema(format!("macro {name} has no argument '{key}'"))
                    .with_path(format!("/{key}"))
                    .with_hint(hint),
            );
        }
    }
    Ok(())
}

pub(super) fn arg_i32(args: &BTreeMap<String, String>, key: &str, default: i32) -> PxlResult<i32> {
    match args.get(key) {
        None => Ok(default),
        Some(v) => v.parse::<i32>().map_err(|_| {
            PxlError::schema(format!("macro arg {key}={v} is not an integer"))
                .with_path(format!("/{key}"))
        }),
    }
}

pub(super) fn arg_str<'a>(
    args: &'a BTreeMap<String, String>,
    key: &str,
    default: &'a str,
) -> &'a str {
    args.get(key).map(|s| s.as_str()).unwrap_or(default)
}

pub(super) fn steps_of(args: &BTreeMap<String, String>, default: i32) -> PxlResult<usize> {
    let n = arg_i32(args, "steps", default)?;
    if !(1..=16).contains(&n) {
        return Err(PxlError::schema(format!("steps {n} is outside 1..=16"))
            .with_hint("an animation has at most 16 frames"));
    }
    Ok(n as usize)
}
