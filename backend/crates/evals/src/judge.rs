//! The LLM judge: yes/no on each criterion of a rubric, with a reason, as
//! structured output `{criterion_id: {met, reason}}`.

use std::sync::Arc;

use application::structured::{check_schema, parse_object};
use domain::llm::{
    LlmMessage, LlmProvider, LlmRequest, LlmRole, MessageRole, OutputFormat, SystemBlock,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::suite::Criterion;

/// The judge runs under this role so that the configuration can give it its
/// own model; no real use case runs as an Interviewer.
pub const JUDGE_ROLE: LlmRole = LlmRole::Interviewer;

const SYSTEM: &str = "You are a strict, fair evaluator of answers written by an AI tutoring system. \
You receive the material (a conversation or an input, and the system's output) and a list of criteria. \
For each criterion decide whether it is met (true) or not (false), judging only what the material shows, \
and give a short reason in Italian (one sentence) that quotes or points to the evidence. \
A criterion that says what the output must NOT do is met when the output does not do it. \
Do not be generous: if the evidence is ambiguous, the criterion is not met. \
Answer only with the JSON object requested.";

#[derive(Debug, Clone, Serialize)]
pub struct CriterionResult {
    pub id: String,
    pub weight: f64,
    pub met: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JudgeReport {
    pub criteria: Vec<CriterionResult>,
    /// Sum of the weights of the met criteria over the sum of all weights.
    /// `None` when the judge could not answer.
    pub score: Option<f64>,
    pub error: Option<String>,
}

pub struct Judge {
    llm: Arc<dyn LlmProvider>,
}

/// JSON schema of the verdict for these criteria (strict subset: every
/// property required, no extras).
#[must_use]
pub fn verdict_schema(criteria: &[Criterion]) -> Value {
    let item = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["met", "reason"],
        "properties": {"met": {"type": "boolean"}, "reason": {"type": "string"}}
    });
    let props: serde_json::Map<String, Value> = criteria
        .iter()
        .map(|c| (c.id.clone(), item.clone()))
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": criteria.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        "properties": props,
    })
}

/// Reads the judge's answer. A criterion the judge left out counts as not met.
pub fn parse_verdict(text: &str, criteria: &[Criterion]) -> Result<Vec<CriterionResult>, String> {
    let value = parse_object(text).map_err(|e| e.join("; "))?;
    let problems = check_schema(&value, &verdict_schema(criteria));
    let lenient = problems.is_empty();
    Ok(criteria
        .iter()
        .map(|c| {
            let entry = value.get(&c.id);
            let met = entry.and_then(|e| e.get("met")).and_then(Value::as_bool);
            let reason = entry
                .and_then(|e| e.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            CriterionResult {
                id: c.id.clone(),
                weight: c.weight,
                met: met.unwrap_or(false),
                reason: if met.is_none() {
                    format!(
                        "the judge did not answer this criterion{}",
                        if lenient { "" } else { " (schema mismatch)" }
                    )
                } else {
                    reason
                },
            }
        })
        .collect())
}

#[must_use]
pub fn score(results: &[CriterionResult]) -> Option<f64> {
    let total: f64 = results.iter().map(|r| r.weight).sum();
    if total <= 0.0 {
        return None;
    }
    Some(
        results
            .iter()
            .filter(|r| r.met)
            .map(|r| r.weight)
            .sum::<f64>()
            / total,
    )
}

impl Judge {
    #[must_use]
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Judges `material` against `criteria`. No criteria: nothing to judge.
    pub async fn judge(&self, material: &str, criteria: &[Criterion]) -> Option<JudgeReport> {
        if criteria.is_empty() {
            return None;
        }
        let list: String = criteria
            .iter()
            .map(|c| format!("- {}: {}", c.id, c.text))
            .collect::<Vec<_>>()
            .join("\n");
        let mut req = LlmRequest::new(JUDGE_ROLE, "eval.judge");
        req.system = vec![SystemBlock {
            text: SYSTEM.to_owned(),
            cache_breakpoint: false,
        }];
        req.messages = vec![LlmMessage::text(
            MessageRole::User,
            format!("MATERIAL\n{material}\n\nCRITERIA\n{list}"),
        )];
        req.output = OutputFormat::JsonSchema {
            name: "judge_verdict".into(),
            schema: verdict_schema(criteria),
        };
        req.max_output_tokens = Some(3000);
        let failed = |error: String| JudgeReport {
            criteria: Vec::new(),
            score: None,
            error: Some(error),
        };
        let response = match self.llm.complete(req).await {
            Ok(r) => r,
            Err(e) => return Some(failed(e.to_string())),
        };
        let text = application::structured::final_text(&response.message);
        Some(match parse_verdict(&text, criteria) {
            Ok(results) => JudgeReport {
                score: score(&results),
                criteria: results,
                error: None,
            },
            Err(e) => failed(format!("unreadable verdict: {e}")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crit() -> Vec<Criterion> {
        vec![
            Criterion {
                id: "a".into(),
                weight: 3.0,
                text: "uno".into(),
            },
            Criterion {
                id: "b".into(),
                weight: 1.0,
                text: "due".into(),
            },
        ]
    }

    #[test]
    fn the_schema_requires_every_criterion() {
        let s = verdict_schema(&crit());
        assert_eq!(s["required"], json!(["a", "b"]));
        assert_eq!(s["properties"]["a"]["required"], json!(["met", "reason"]));
    }

    #[test]
    fn parses_a_verdict_and_scores_by_weight() {
        let text = r#"{"a": {"met": true, "reason": "ok"}, "b": {"met": false, "reason": "no"}}"#;
        let r = parse_verdict(text, &crit()).unwrap();
        assert!(r[0].met && !r[1].met);
        assert_eq!(score(&r), Some(0.75));
    }

    #[test]
    fn a_missing_criterion_is_not_met_and_prose_around_json_is_tolerated() {
        let r = parse_verdict(
            "Ecco: {\"a\": {\"met\": true, \"reason\": \"ok\"}}",
            &crit(),
        )
        .unwrap();
        assert!(r[0].met);
        assert!(!r[1].met);
        assert!(r[1].reason.contains("did not answer"));
        assert!(parse_verdict("niente", &crit()).is_err());
    }
}
