//! Prices per model, from `backend/prompts/llm-port.md` §9 and §13.6.

use domain::llm::Usage;

/// USD per million tokens (input, output).
const TABLE: [(&str, f64, f64); 3] = [
    ("claude-sonnet-5-5", 2.0, 10.0),
    ("claude-opus-5-5", 4.0, 20.0),
    ("claude-haiku-5-5", 0.10, 0.50),
];
/// Cache reads cost 0.1x the input price, cache writes 1.25x (`llm-port.md` §13.6).
const CACHE_READ: f64 = 0.1;
const CACHE_WRITE: f64 = 1.25;
/// One server-side web search (`evals/README.md`: 10 USD per 1000).
const WEB_SEARCH_USD: f64 = 0.01;

/// `anthropic/claude-sonnet-5.5` (OpenRouter slug) -> `claude-sonnet-5-5`.
#[must_use]
pub fn normalize_model(model: &str) -> String {
    model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .replace('.', "-")
        .to_lowercase()
}

/// A cost and whether the model was missing from the table. An unknown model
/// is priced like the most expensive one, so a budget errs on the safe side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cost {
    pub usd: f64,
    pub price_assumed: bool,
}

#[must_use]
pub fn cost(model: &str, usage: &Usage) -> Cost {
    let name = normalize_model(model);
    let known = TABLE.iter().find(|(m, _, _)| *m == name);
    let (input, output) = known.map_or((4.0, 20.0), |(_, i, o)| (*i, *o));
    let tokens = f64::from(usage.input_tokens) * input
        + f64::from(usage.output_tokens) * output
        + f64::from(usage.cache_read_tokens) * input * CACHE_READ
        + f64::from(usage.cache_write_tokens) * input * CACHE_WRITE;
    Cost {
        usd: tokens / 1_000_000.0 + f64::from(usage.web_searches) * WEB_SEARCH_USD,
        price_assumed: known.is_none(),
    }
}

/// Typical tokens (input, output) of one call, for `--dry-run` and for the
/// check before a run starts. From `llm-port.md` §13.6 and `evals/README.md`.
#[must_use]
pub fn typical_cost(model: &str, input_tokens: u32, output_tokens: u32, searches: u32) -> f64 {
    cost(
        model,
        &Usage {
            input_tokens,
            output_tokens,
            web_searches: searches,
            ..Usage::default()
        },
    )
    .usd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_normalized() {
        assert_eq!(
            normalize_model("anthropic/claude-sonnet-5.5"),
            "claude-sonnet-5-5"
        );
        assert_eq!(normalize_model("claude-haiku-5-5"), "claude-haiku-5-5");
    }

    #[test]
    fn cost_follows_the_table() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            thinking_tokens: 400_000, // already inside output_tokens: not billed twice
            cache_read_tokens: 1_000_000,
            cache_write_tokens: 0,
            web_searches: 2,
            // No price of its own: the pages cost input tokens only (ADR 0009).
            web_fetches: 3,
        };
        let c = cost("claude-sonnet-5-5", &usage);
        assert!((c.usd - (2.0 + 10.0 + 0.2 + 0.02)).abs() < 1e-9);
        assert!(!c.price_assumed);
    }

    #[test]
    fn an_unknown_model_is_priced_high_and_flagged() {
        let usage = Usage {
            input_tokens: 1_000_000,
            ..Usage::default()
        };
        let c = cost("some/other-model", &usage);
        assert!((c.usd - 4.0).abs() < 1e-9);
        assert!(c.price_assumed);
    }
}
