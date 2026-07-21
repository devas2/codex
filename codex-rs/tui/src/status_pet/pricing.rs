//! Codex / OpenAI model pricing for session cost estimates.
//!
//! Rates are standard short-context USD per 1M tokens from the official
//! OpenAI API pricing page. Cache-write pricing:
//! - GPT-5.6 and later: 1.25× uncached input
//! - Pre-GPT-5.6: no additional fee beyond input (write tokens priced at input)

use crate::token_usage::TokenUsage;

/// USD per 1M tokens for one model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ModelRates {
    pub input_per_m: f64,
    pub cached_input_per_m: f64,
    /// Cache-write rate per 1M. For pre-5.6 this equals `input_per_m`.
    pub cache_write_per_m: f64,
    pub output_per_m: f64,
}

impl ModelRates {
    const fn new(input: f64, cached: f64, cache_write: f64, output: f64) -> Self {
        Self {
            input_per_m: input,
            cached_input_per_m: cached,
            cache_write_per_m: cache_write,
            output_per_m: output,
        }
    }

    /// Pre-5.6 models: cache writes cost the normal input rate (no extra fee).
    const fn pre56(input: f64, cached: f64, output: f64) -> Self {
        Self::new(input, cached, input, output)
    }

    /// GPT-5.6+: cache writes at 1.25× input.
    const fn gpt56(input: f64, cached: f64, output: f64) -> Self {
        Self::new(input, cached, input * 1.25, output)
    }
}

/// Default fallback rates (gpt-5.4 standard).
pub(crate) const FALLBACK_RATES: ModelRates = ModelRates::pre56(2.50, 0.25, 15.00);

/// Normalize a model slug for pricing lookup (strip provider prefix).
pub(crate) fn normalize_model_slug(model: &str) -> &str {
    model
        .strip_prefix("openai.")
        .unwrap_or(model)
        .split('/')
        .next_back()
        .unwrap_or(model)
}

/// Look up standard short-context rates for a model slug.
pub(crate) fn rates_for_model(model: &str) -> ModelRates {
    let slug = normalize_model_slug(model);
    // Exact matches first.
    match slug {
        "gpt-5.6-sol" => ModelRates::gpt56(5.00, 0.50, 30.00),
        "gpt-5.6-terra" => ModelRates::gpt56(2.50, 0.25, 15.00),
        "gpt-5.6-luna" => ModelRates::gpt56(1.00, 0.10, 6.00),
        "gpt-5.5" | "gpt-5.5-pro" => ModelRates::pre56(5.00, 0.50, 30.00),
        "gpt-5.4" => ModelRates::pre56(2.50, 0.25, 15.00),
        "gpt-5.4-mini" => ModelRates::pre56(0.75, 0.075, 4.50),
        "gpt-5.4-nano" => ModelRates::pre56(0.20, 0.02, 1.25),
        "gpt-5.3-codex" => ModelRates::pre56(1.75, 0.175, 14.00),
        "gpt-5.2" | "gpt-5.2-codex" => ModelRates::pre56(1.75, 0.175, 14.00),
        "gpt-5.1-codex" | "gpt-5.1-codex-max" => ModelRates::pre56(1.25, 0.125, 10.00),
        _ => {
            // Prefix heuristics for future codex / 5.6 family models.
            if slug.contains("5.6") || slug.starts_with("gpt-5.6") {
                // Unknown 5.6 variant: use sol rates as conservative upper bound for sol-class,
                // but terra-like middle is safer for "unknown" — use terra.
                ModelRates::gpt56(2.50, 0.25, 15.00)
            } else if slug.contains("codex") {
                ModelRates::pre56(1.75, 0.175, 14.00)
            } else {
                FALLBACK_RATES
            }
        }
    }
}

/// Estimate session USD cost from accumulated token usage and model rates.
///
/// Assumes OpenAI semantics: `cached_input_tokens` and `cache_write_input_tokens`
/// are subsets of `input_tokens`. Reasoning is already included in `output_tokens`.
pub(crate) fn estimate_session_cost_usd(model: &str, usage: &TokenUsage) -> f64 {
    let rates = rates_for_model(model);
    estimate_cost_with_rates(usage, rates)
}

pub(crate) fn estimate_cost_with_rates(usage: &TokenUsage, rates: ModelRates) -> f64 {
    let uncached = usage.uncached_input() as f64;
    let cached = usage.cached_input() as f64;
    let cache_write = usage.cache_write_input() as f64;
    let output = usage.output_tokens.max(0) as f64;

    (uncached * rates.input_per_m
        + cached * rates.cached_input_per_m
        + cache_write * rates.cache_write_per_m
        + output * rates.output_per_m)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sol_rates_include_125_write() {
        let r = rates_for_model("gpt-5.6-sol");
        assert!((r.input_per_m - 5.0).abs() < 1e-9);
        assert!((r.cache_write_per_m - 6.25).abs() < 1e-9);
        assert!((r.cached_input_per_m - 0.50).abs() < 1e-9);
        assert!((r.output_per_m - 30.0).abs() < 1e-9);
    }

    #[test]
    fn pre56_write_equals_input() {
        let r = rates_for_model("gpt-5.3-codex");
        assert!((r.cache_write_per_m - r.input_per_m).abs() < 1e-9);
        assert!((r.input_per_m - 1.75).abs() < 1e-9);
    }

    #[test]
    fn strips_openai_prefix() {
        let a = rates_for_model("openai.gpt-5.6-sol");
        let b = rates_for_model("gpt-5.6-sol");
        assert_eq!(a, b);
    }

    #[test]
    fn cost_with_cache_write_on_sol() {
        // input=100, cached=40, write=60 → uncached=0; output=10
        // cost = 0*5 + 40*0.5 + 60*6.25 + 10*30 = 0 + 20 + 375 + 300 = 695 / 1e6
        let usage = TokenUsage {
            input_tokens: 100,
            cached_input_tokens: 40,
            cache_write_input_tokens: 60,
            output_tokens: 10,
            reasoning_output_tokens: 5,
            total_tokens: 110,
        };
        let cost = estimate_session_cost_usd("gpt-5.6-sol", &usage);
        let expected = (40.0 * 0.50 + 60.0 * 6.25 + 10.0 * 30.0) / 1_000_000.0;
        assert!((cost - expected).abs() < 1e-12, "cost={cost} expected={expected}");
    }

    #[test]
    fn cache_write_increases_cost_on_sol() {
        let base = TokenUsage {
            input_tokens: 100,
            cached_input_tokens: 0,
            cache_write_input_tokens: 0,
            output_tokens: 10,
            reasoning_output_tokens: 0,
            total_tokens: 110,
        };
        let with_write = TokenUsage {
            input_tokens: 100,
            cached_input_tokens: 0,
            cache_write_input_tokens: 60,
            output_tokens: 10,
            reasoning_output_tokens: 0,
            total_tokens: 110,
        };
        let c0 = estimate_session_cost_usd("gpt-5.6-sol", &base);
        let c1 = estimate_session_cost_usd("gpt-5.6-sol", &with_write);
        assert!(c1 > c0, "write should raise cost: {c1} vs {c0}");
    }

    #[test]
    fn uncached_formula() {
        let usage = TokenUsage {
            input_tokens: 100,
            cached_input_tokens: 40,
            cache_write_input_tokens: 60,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 100,
        };
        assert_eq!(usage.uncached_input(), 0);
        assert_eq!(usage.billable_tokens(), 100);
    }
}
