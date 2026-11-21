use serde_json::Value;

/// Fields the app must never receive (contract §7).
const RESERVED_FIELD: &str = "rubric";

/// Returns the document as the app may see it: `rubric` removed from every
/// block object, including blocks nested in any `fallback`.
/// Everything else is passed through untouched, so unknown blocks survive.
#[must_use]
pub fn client_view(document: &Value) -> Value {
    let mut view = document.clone();
    if let Some(blocks) = view.get_mut("blocks") {
        strip_blocks(blocks);
    }
    view
}

fn strip_blocks(blocks: &mut Value) {
    let Some(list) = blocks.as_array_mut() else {
        return;
    };
    for block in list {
        let Some(obj) = block.as_object_mut() else {
            continue;
        };
        // Defense in depth: strip from every block object, whatever its type,
        // so a malformed or future block cannot leak a rubric.
        obj.remove(RESERVED_FIELD);
        if let Some(fallback) = obj.get_mut("fallback") {
            strip_blocks(fallback);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn removes_rubric_at_top_level_and_in_fallback() {
        let doc = json!({"title": "t", "blocks": [
            {"id": "a", "type": "open_answer", "question": "q", "rubric": "secret"},
            {"id": "b", "type": "missing_component", "fallback": [
                {"id": "c", "type": "open_answer", "rubric": "secret"}
            ]},
            {"id": "d", "type": "paragraph", "rubric": "leak"},
            {"id": "e", "type": "open_answer", "rubric": "secret", "fallback": [
                {"type": "missing_component", "rubric": "x", "fallback": [{"rubric": "deep"}]},
                "not an object", 7
            ]},
            {"type": 5, "rubric": "malformed"}
        ]});
        let view = client_view(&doc);
        assert!(view["blocks"][0].get("rubric").is_none());
        assert_eq!(view["blocks"][0]["question"], "q");
        assert!(view["blocks"][1]["fallback"][0].get("rubric").is_none());
        assert!(view["blocks"][2].get("rubric").is_none(), "any block type");
        assert_eq!(doc["blocks"][0]["rubric"], "secret", "input untouched");
    }

    #[test]
    fn strips_deeply_nested_and_malformed_blocks() {
        let doc = json!({"blocks": [
            {"id": "e", "fallback": [{"fallback": [{"rubric": "deep"}], "rubric": "x"}, 7]},
            {"type": 5, "rubric": "malformed"}
        ]});
        let text = client_view(&doc).to_string();
        assert!(!text.contains("rubric"), "{text}");
    }

    #[test]
    fn tolerates_malformed_input() {
        assert_eq!(client_view(&json!(null)), json!(null));
        assert_eq!(client_view(&json!({"blocks": 3})), json!({"blocks": 3}));
    }
}
