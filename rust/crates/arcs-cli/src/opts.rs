//! `--opts '{"iterations":800}'`, the CLI half of the TS agent opts bag.
//!
//! [`arcs_agents::AgentOpts`] is a typed struct, not a `Record<string,
//! unknown>`, and its doc comment says why: "a typo in a key should not
//! silently produce a differently-configured bot". A JSON front end would give
//! that back for free unless it is strict, so this parser **rejects unknown
//! keys** rather than ignoring them. A run whose configuration silently
//! differed from what the command line asked for is exactly the class of
//! mistake FINDINGS records as costing real measurement time.
//!
//! Keys are the TS camelCase names (`rolloutDepth`, `cPuct`, …) so a command
//! line copied out of the TypeScript lab notes still means the same thing.

use arcs_agents::{AgentOpts, BattleValuation};
use serde_json::Value;

pub fn parse_agent_opts(json: &str) -> Result<AgentOpts, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("--opts is not JSON: {e}"))?;
    let map = value
        .as_object()
        .ok_or_else(|| "--opts must be a JSON object".to_string())?;

    let mut opts = AgentOpts::default();
    for (key, v) in map {
        match key.as_str() {
            "settle" => opts.settle = Some(as_bool(key, v)?),
            "samples" => opts.samples = Some(as_usize(key, v)?),
            "battles" => {
                opts.battles = Some(match v.as_str() {
                    Some("sample") => BattleValuation::Sample,
                    Some("exact") => BattleValuation::Exact,
                    _ => return Err("`battles` must be \"sample\" or \"exact\"".to_string()),
                })
            }
            "battleMass" => opts.battle_mass = Some(as_f64(key, v)?),
            "iterations" => opts.iterations = Some(as_usize(key, v)?),
            "timeMs" => opts.time_ms = Some(as_usize(key, v)? as u64),
            "cPuct" => opts.c_puct = Some(as_f64(key, v)?),
            "c" => opts.c = Some(as_f64(key, v)?),
            "maxActions" => opts.max_actions = Some(as_usize(key, v)?),
            "maxDepth" => opts.max_depth = Some(as_usize(key, v)?),
            "rolloutDepth" | "depth" => opts.rollout_depth = Some(as_usize(key, v)?),
            "worlds" => opts.worlds = Some(as_usize(key, v)?),
            "priorTemp" => opts.prior_temp = Some(as_f64(key, v)?),
            "priors" => opts.priors = Some(as_bool(key, v)?),
            "rolloutLeaf" => opts.rollout_leaf = Some(as_bool(key, v)?),
            "candidates" => opts.candidates = Some(as_bool(key, v)?),
            "rollouts" => opts.rollouts = Some(as_usize(key, v)?),
            other => return Err(format!("unknown agent option `{other}`")),
        }
    }
    Ok(opts)
}

fn as_bool(key: &str, v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| format!("`{key}` must be a bool"))
}

fn as_usize(key: &str, v: &Value) -> Result<usize, String> {
    v.as_u64()
        .map(|n| n as usize)
        .ok_or_else(|| format!("`{key}` must be a non-negative integer"))
}

fn as_f64(key: &str, v: &Value) -> Result<f64, String> {
    v.as_f64()
        .ok_or_else(|| format!("`{key}` must be a number"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_ts_key_names() {
        let opts = parse_agent_opts(r#"{"iterations":800,"rolloutDepth":20,"battles":"exact"}"#)
            .expect("parses");
        assert_eq!(opts.iterations, Some(800));
        assert_eq!(opts.rollout_depth, Some(20));
        assert_eq!(opts.battles, Some(BattleValuation::Exact));
        // Everything untouched stays at the registry entry's own default.
        assert_eq!(opts.weights, None);
    }

    #[test]
    fn a_typo_is_an_error_not_a_shrug() {
        assert!(parse_agent_opts(r#"{"iteration":800}"#).is_err());
        assert!(parse_agent_opts(r#"{"iterations":"lots"}"#).is_err());
    }
}
