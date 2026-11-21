#![allow(
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::cast_possible_truncation
)] // test-only probe tables
//! Runs the Validator over the shared contract fixtures in
//! `docs/contracts/fixtures/` (fixture mode: no Topic context).

use std::fs;
use std::path::{Path, PathBuf};

use domain::{LessonValidator, ValidationContext, ValidatorConfig, client_view};
use serde_json::{Value, json};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/contracts/fixtures")
}

fn load(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn validate(doc: &Value) -> Vec<domain::LessonIssue> {
    LessonValidator::new(ValidatorConfig::default(), ValidationContext::fixture()).validate(doc)
}

fn files(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            n.starts_with(prefix)
                && p.extension().is_some_and(|e| e == "json")
                && !n.ends_with(".expected.json")
        })
        .collect();
    v.sort();
    v
}

#[test]
fn valid_fixtures_pass() {
    let found = files(&fixtures_dir(), "lesson-");
    assert!(found.len() >= 4, "fixtures not found in {found:?}");
    for path in found {
        let issues = validate(&load(&path));
        assert!(issues.is_empty(), "{}: {issues:#?}", path.display());
    }
}

#[test]
fn unknown_type_fixture_yields_only_unknown_block_type() {
    let issues = validate(&load(&fixtures_dir().join("unknown-type.json")));
    let codes: Vec<_> = issues.iter().map(|i| i.code.as_str()).collect();
    assert_eq!(codes, ["unknown_block_type"], "{issues:#?}");
}

#[test]
fn invalid_fixtures_yield_exactly_the_expected_codes() {
    let dir = fixtures_dir().join("invalid");
    let found = files(&dir, "");
    assert!(found.len() >= 19, "invalid fixtures not found");
    for path in found {
        let expected_path = path.with_extension("expected.json");
        let mut expected: Vec<String> =
            serde_json::from_value(load(&expected_path)).expect("expected codes");
        let issues = validate(&load(&path));
        let mut got: Vec<String> = issues.iter().map(|i| i.code.as_str().to_owned()).collect();
        got.sort();
        got.dedup();
        expected.sort();
        assert_eq!(got, expected, "{}: {issues:#?}", path.display());
    }
}

#[test]
fn client_view_of_fixtures_has_no_rubric() {
    for path in files(&fixtures_dir(), "lesson-") {
        let view = client_view(&load(&path)).to_string();
        assert!(!view.contains("\"rubric\""), "{}", path.display());
    }
    let original = load(&fixtures_dir().join("lesson-photo-exposure.json")).to_string();
    assert!(
        original.contains("\"rubric\""),
        "fixture should carry a rubric"
    );
}

/// Parser limits must match `sdui-schema-v1.json` (the schema is the source
/// for the app and for the Generator's structured output). Probes each
/// numeric bound at its edge: a document just inside must pass the parser, a
/// document just outside must fail it.
#[test]
fn parser_limits_match_json_schema() {
    let schema = load(&fixtures_dir().join("../sdui-schema-v1.json"));
    let top = &schema["properties"];
    let defs = &schema["$defs"];
    let base = load(&fixtures_dir().join("lesson-git-staging.json"));

    let bound = |v: &Value, k: &str| {
        v[k].as_u64()
            .unwrap_or_else(|| panic!("{k} missing in {v}"))
    };
    let schema_min_minutes = bound(&top["estimated_minutes"], "minimum");
    let schema_max_minutes = bound(&top["estimated_minutes"], "maximum");
    let title_max = bound(&top["title"], "maxLength");
    let objective_max = bound(&top["objective"], "maxLength");
    let glossary_max = bound(&top["glossary"], "maxItems");
    let sources_max = bound(&top["sources"], "maxItems");
    let blocks_min = bound(&top["blocks"], "minItems");
    let blocks_max = bound(&top["blocks"], "maxItems");
    let para_max = bound(&defs["paragraph"]["properties"]["text"], "maxLength");
    let callout_max = bound(&defs["callout"]["properties"]["text"], "maxLength");
    let code_max = bound(&defs["code"]["properties"]["code"], "maxLength");
    let fig = &defs["diagram"]["properties"];
    let fig_title_max = bound(&fig["title"], "maxLength");
    let fig_nodes_min = bound(&fig["nodes"], "minItems");
    let fig_nodes_max = bound(&fig["nodes"], "maxItems");
    let fig_edges_max = bound(&fig["edges"], "maxItems");
    let node_props = &fig["nodes"]["items"]["properties"];
    let node_label_max = bound(&node_props["label"], "maxLength");
    let node_detail_max = bound(&node_props["detail"], "maxLength");
    let edge_label_max = bound(&fig["edges"]["items"]["properties"]["label"], "maxLength");
    let fig_caption_max = bound(&fig["caption"], "maxLength");
    let fig_alt_max = bound(&fig["alt"], "maxLength");

    let schema_errors = |doc: &Value| -> bool {
        validate(doc)
            .iter()
            .any(|i| i.code == domain::LessonErrorCode::Schema)
    };
    let with = |f: &dyn Fn(&mut Value)| {
        let mut d = base.clone();
        f(&mut d);
        d
    };
    let text = |n: u64| "a".repeat(usize::try_from(n).expect("small"));

    // Appends a minimal valid diagram and lets `f` edit it. Graph rules are
    // `diagram_structure`, not `schema`, so only the bounds are probed here.
    let diagram = |d: &mut Value, f: &dyn Fn(&mut Value)| {
        let mut fig = json!({
            "id": "fig", "type": "diagram", "layout": "flow",
            "nodes": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"}],
            "edges": [{"from": "a", "to": "b"}],
            "caption": "Caption.", "alt": "Alt."
        });
        f(&mut fig);
        d["blocks"].as_array_mut().unwrap().push(fig);
    };
    let nodes_of = |n: u64| {
        Value::Array(
            (0..n)
                .map(|i| json!({"id": format!("n{i}"), "label": "L"}))
                .collect(),
        )
    };

    let probes: Vec<(&str, u64, Box<dyn Fn(&mut Value, u64)>)> = vec![
        (
            "diagram.title",
            fig_title_max,
            Box::new(move |d, n| diagram(d, &|f| f["title"] = json!(text(n)))),
        ),
        (
            "diagram.nodes[].label",
            node_label_max,
            Box::new(move |d, n| diagram(d, &|f| f["nodes"][0]["label"] = json!(text(n)))),
        ),
        (
            "diagram.nodes[].detail",
            node_detail_max,
            Box::new(move |d, n| diagram(d, &|f| f["nodes"][0]["detail"] = json!(text(n)))),
        ),
        (
            "diagram.edges[].label",
            edge_label_max,
            Box::new(move |d, n| diagram(d, &|f| f["edges"][0]["label"] = json!(text(n)))),
        ),
        (
            "diagram.caption",
            fig_caption_max,
            Box::new(move |d, n| diagram(d, &|f| f["caption"] = json!(text(n)))),
        ),
        (
            "diagram.alt",
            fig_alt_max,
            Box::new(move |d, n| diagram(d, &|f| f["alt"] = json!(text(n)))),
        ),
        (
            "diagram.nodes count max",
            fig_nodes_max,
            Box::new(move |d, n| diagram(d, &|f| f["nodes"] = nodes_of(n))),
        ),
        (
            "diagram.edges count max",
            fig_edges_max,
            Box::new(move |d, n| {
                diagram(d, &|f| {
                    f["edges"] =
                        Value::Array((0..n).map(|_| json!({"from": "a", "to": "b"})).collect());
                });
            }),
        ),
        (
            "title",
            title_max,
            Box::new(|d, n| d["title"] = json!(text(n))),
        ),
        (
            "objective",
            objective_max,
            Box::new(|d, n| d["objective"] = json!(text(n))),
        ),
        (
            "estimated_minutes max",
            schema_max_minutes,
            Box::new(|d, n| d["estimated_minutes"] = json!(n)),
        ),
        (
            "paragraph.text",
            para_max,
            Box::new(|d, n| {
                let b = d["blocks"].as_array_mut().unwrap();
                let p = b.iter_mut().find(|b| b["type"] == "paragraph").unwrap();
                p["text"] = json!(text(n));
            }),
        ),
        (
            "callout.text",
            callout_max,
            Box::new(|d, n| {
                let b = d["blocks"].as_array_mut().unwrap();
                let p = b.iter_mut().find(|b| b["type"] == "callout").unwrap();
                p["text"] = json!(text(n));
            }),
        ),
        (
            "code.code",
            code_max,
            Box::new(|d, n| {
                let b = d["blocks"].as_array_mut().unwrap();
                let p = b.iter_mut().find(|b| b["type"] == "code").unwrap();
                p["code"] = json!(text(n));
            }),
        ),
    ];
    for (name, max, set) in &probes {
        assert!(
            !schema_errors(&with(&|d| set(d, *max))),
            "{name}: {max} rejected"
        );
        assert!(
            schema_errors(&with(&|d| set(d, max + 1))),
            "{name}: {} accepted",
            max + 1
        );
    }
    assert!(!schema_errors(&with(
        &|d| d["estimated_minutes"] = json!(schema_min_minutes)
    )));
    assert!(schema_errors(&with(
        &|d| d["estimated_minutes"] = json!(schema_min_minutes - 1)
    )));

    // Lower bound of the node count: the minimum passes, one less does not.
    let few = |n: u64| {
        with(&|d| {
            diagram(d, &|f| {
                f["nodes"] = nodes_of(n);
                f["edges"] = json!([]);
            });
        })
    };
    assert!(!schema_errors(&few(fig_nodes_min)));
    assert!(schema_errors(&few(fig_nodes_min - 1)));

    // Array sizes: 60 blocks ok, 61 not; 2 blocks not.
    let padded = |n: u64| {
        with(&|d| {
            let b = d["blocks"].as_array_mut().unwrap();
            let filler = json!({"id": "pad", "type": "paragraph", "text": "ok"});
            b.truncate(usize::try_from(blocks_min).unwrap());
            while (b.len() as u64) < n {
                let mut f = filler.clone();
                f["id"] = json!(format!("pad{}", b.len()));
                b.push(f);
            }
            b.truncate(usize::try_from(n).unwrap());
        })
    };
    assert!(!schema_errors(&padded(blocks_max)));
    assert!(schema_errors(&padded(blocks_max + 1)));
    assert!(schema_errors(&padded(blocks_min - 1)));

    let entry = |i: usize| json!({"key": format!("k{i}"), "term": "t", "definition": "d"});
    let glossary =
        |n: u64| with(&|d| d["glossary"] = Value::Array((0..n as usize).map(entry).collect()));
    assert!(!schema_errors(&glossary(glossary_max)));
    assert!(schema_errors(&glossary(glossary_max + 1)));
    let src = |i: usize| json!({"key": format!("s{i}"), "resource_id": null, "title": "t", "url": "https://a.b"});
    let sources =
        |n: u64| with(&|d| d["sources"] = Value::Array((0..n as usize).map(src).collect()));
    assert!(!schema_errors(&sources(sources_max)));
    assert!(schema_errors(&sources(sources_max + 1)));
}
