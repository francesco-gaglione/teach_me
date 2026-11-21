//! Reading the `*.cases.json` suites (format: `backend/evals/README.md`).

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Tutor,
    Researcher,
    Planner,
    Generator,
    Grader,
    Recorder,
}

impl Role {
    pub fn parse(dir: &str) -> Option<Self> {
        match dir {
            "tutor" => Some(Self::Tutor),
            "researcher" => Some(Self::Researcher),
            "planner" => Some(Self::Planner),
            "generator" => Some(Self::Generator),
            "grader" => Some(Self::Grader),
            "recorder" => Some(Self::Recorder),
            _ => None,
        }
    }
}

/// One criterion of a rubric, judged yes/no.
#[derive(Debug, Clone, PartialEq)]
pub struct Criterion {
    pub id: String,
    pub weight: f64,
    pub text: String,
}

/// The host of an `https://` address, lowercased, or `None` for anything else.
fn https_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// What a suite or a case needs from the environment to be run at all
/// (`requires`). A run without it is skipped and says why, never failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Requires {
    /// The Generator's web fetch tool is on and the provider can run it.
    pub web_fetch: bool,
    /// A page served locally in place of the network; the runner cannot do it.
    pub fixture_pages: bool,
}

impl Requires {
    fn parse(place: &str, v: Option<&Value>) -> anyhow::Result<Self> {
        let Some(v) = v else {
            return Ok(Self::default());
        };
        let Some(o) = v.as_object() else {
            bail!("`requires` in {place} must be an object");
        };
        let mut out = Self::default();
        for (key, value) in o {
            let Some(flag) = value.as_bool() else {
                bail!("`requires.{key}` in {place} must be true or false, found {value}");
            };
            match key.as_str() {
                "web_fetch" => out.web_fetch = flag,
                "fixture_pages" => out.fixture_pages = flag,
                other => bail!(
                    "unknown requirement `{other}` in {place}: known are `web_fetch` and `fixture_pages`"
                ),
            }
        }
        Ok(out)
    }

    #[must_use]
    pub fn or(self, other: Self) -> Self {
        Self {
            web_fetch: self.web_fetch || other.web_fetch,
            fixture_pages: self.fixture_pages || other.fixture_pages,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    pub raw: Value,
}

impl Case {
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.raw.get(key)
    }

    #[must_use]
    pub fn str_of(&self, key: &str) -> &str {
        self.raw
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// Language the case's topic is written in: `content_language`, `it`
    /// when absent. `Suite::parse` refuses any other value, so the fallback
    /// is only the default.
    #[must_use]
    pub fn content_language(&self) -> domain::ContentLanguage {
        self.raw
            .get("content_language")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn array(&self, key: &str) -> Vec<Value> {
        self.raw
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct Suite {
    /// `<role dir>/<file stem>`, e.g. `tutor/conversation`. What `--suite` takes.
    pub id: String,
    pub role: Role,
    pub defaults: Value,
    pub pass_criteria: Value,
    pub cases: Vec<Case>,
    /// `requires` of the whole suite.
    pub requires: Requires,
}

impl Suite {
    pub fn parse(id: &str, text: &str) -> anyhow::Result<Self> {
        let root: Value = serde_json::from_str(text).context("not valid JSON")?;
        let role_dir = id.split('/').next().unwrap_or_default();
        let Some(role) = Role::parse(role_dir) else {
            bail!("unknown role directory `{role_dir}`");
        };
        let cases = root
            .get("cases")
            .and_then(Value::as_array)
            .context("`cases` is missing")?
            .iter()
            .map(|raw| {
                let id = raw
                    .get("id")
                    .and_then(Value::as_str)
                    .context("a case has no `id`")?;
                if let Some(v) = raw.get("content_language")
                    && v.as_str()
                        .is_none_or(|s| s.parse::<domain::ContentLanguage>().is_err())
                {
                    bail!("case `{id}`: `content_language` must be \"it\" or \"en\", got {v}");
                }
                Ok(Case {
                    id: id.to_owned(),
                    raw: raw.clone(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let suite = Self {
            id: id.to_owned(),
            role,
            defaults: root.get("defaults").cloned().unwrap_or(Value::Null),
            pass_criteria: root.get("pass_criteria").cloned().unwrap_or(Value::Null),
            cases,
            requires: Requires::parse("the suite", root.get("requires"))?,
        };
        suite.reject_unknown_checks()?;
        suite.reject_bad_fetch_fields()?;
        Ok(suite)
    }

    /// A check the runner does not know is an error at load time: a typo or a
    /// check nobody implemented would otherwise pass unseen. Only the
    /// Planner and the Generator list their keys; the other roles still
    /// report an unknown key as `skipped` in the run.
    fn reject_unknown_checks(&self) -> anyhow::Result<()> {
        let known = |key: &str| match self.role {
            Role::Planner => crate::roles::planner::KNOWN_KEYS.contains(&key),
            Role::Generator => crate::roles::generator::knows_check(key),
            _ => true,
        };
        let keys = |v: Option<&Value>| -> Vec<String> {
            v.and_then(Value::as_object)
                .map_or_default(|o| o.keys().cloned().collect())
        };
        let mut found = vec![(
            "defaults".to_owned(),
            keys(self.defaults.get("deterministic")),
        )];
        found.extend(
            self.cases
                .iter()
                .map(|c| (format!("case `{}`", c.id), keys(c.get("deterministic")))),
        );
        for (place, names) in found {
            if let Some(unknown) = names.iter().find(|k| !known(k)) {
                bail!("unknown check `{unknown}` in {place}: the runner does not compute it");
            }
        }
        let values = std::iter::once(("defaults".to_owned(), self.defaults.get("deterministic")))
            .chain(
                self.cases
                    .iter()
                    .map(|c| (format!("case `{}`", c.id), c.get("deterministic"))),
            );
        for (place, spec) in values {
            for (key, v) in spec.and_then(Value::as_object).into_iter().flatten() {
                let problem = match self.role {
                    Role::Planner => crate::roles::planner::value_problem(key, v),
                    Role::Generator => crate::roles::generator::value_problem(key, v),
                    _ => None,
                };
                if let Some(problem) = problem {
                    bail!("malformed check in {place}: {problem}");
                }
            }
        }
        Ok(())
    }

    /// The web fetch fields (`requires`, `web_fetch`, `fetch_checks`,
    /// `report_metrics`, `fixture_pages`) are read like the checks: one that
    /// cannot be understood is an error at load, not a check that never ran.
    fn reject_bad_fetch_fields(&self) -> anyhow::Result<()> {
        let mut places = vec![("defaults".to_owned(), self.defaults.get("fetch_checks"))];
        places.extend(
            self.cases
                .iter()
                .map(|c| (format!("case `{}`", c.id), c.get("fetch_checks"))),
        );
        for (place, spec) in places {
            let Some(spec) = spec else { continue };
            if self.role != Role::Generator {
                bail!("`fetch_checks` in {place}: only the Generator has them");
            }
            let Some(object) = spec.as_object() else {
                bail!("`fetch_checks` in {place} must be an object");
            };
            for (key, v) in object {
                if !crate::roles::fetch::KEYS.contains(&key.as_str()) {
                    bail!(
                        "unknown fetch check `{key}` in {place}: known are {}",
                        crate::roles::fetch::KEYS.join(", ")
                    );
                }
                if let Some(problem) = crate::roles::fetch::value_problem(key, v) {
                    bail!("malformed fetch check in {place}: {problem}");
                }
            }
        }
        for case in &self.cases {
            let place = format!("case `{}`", case.id);
            Requires::parse(&place, case.get("requires"))?;
            if let Some(v) = case.get("web_fetch")
                && v.as_str() != Some("off")
            {
                bail!("`web_fetch` in {place} must be \"off\", found {v}");
            }
            if let Some(v) = case.get("report_metrics")
                && let Some(problem) = crate::roles::fetch::metrics_problem(v)
            {
                bail!("malformed `report_metrics` in {place}: {problem}");
            }
            if let Some(v) = case.get("fixture_pages")
                && !v
                    .as_object()
                    .is_some_and(|o| o.values().all(Value::is_string))
            {
                bail!("`fixture_pages` in {place} must map URLs to page texts");
            }
            if let Some(pages) = case.get("fixture_pages").and_then(Value::as_object) {
                let source_urls: Vec<&str> = case
                    .raw
                    .get("sources")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s["url"].as_str())
                    .collect();
                for url in pages.keys() {
                    if https_host(url).is_none() {
                        bail!("`fixture_pages` in {place}: `{url}` must be an https address");
                    }
                    if !source_urls.contains(&url.as_str()) {
                        bail!(
                            "`fixture_pages` in {place}: `{url}` is not the `url` of any of the case's `sources`"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// What `case` needs, its suite's needs included.
    #[must_use]
    pub fn requires_for(&self, case: &Case) -> Requires {
        // Validated when the suite loaded.
        self.requires
            .or(Requires::parse(&case.id, case.get("requires")).unwrap_or_default())
    }

    /// Whether the case runs with the web fetch tool off even when the
    /// configuration turns it on (`"web_fetch": "off"`).
    #[must_use]
    pub fn web_fetch_off(&self, case: &Case) -> bool {
        case.get("web_fetch").and_then(Value::as_str) == Some("off")
    }

    /// The checks on what the web fetch tool did: the suite's defaults
    /// overridden key by key by the case's own.
    #[must_use]
    pub fn fetch_checks(&self, case: &Case) -> Map<String, Value> {
        merged(self.defaults.get("fetch_checks"), case.get("fetch_checks"))
    }

    /// Repetitions per case; `override_runs` (`--runs`) wins over the suite.
    #[must_use]
    pub fn runs_per_case(&self, override_runs: Option<u32>) -> u32 {
        override_runs.unwrap_or_else(|| {
            self.defaults
                .get("runs_per_case")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(1)
        })
    }

    /// A key of `defaults` (e.g. `search_budget`).
    #[must_use]
    pub fn default_u32(&self, key: &str) -> Option<u32> {
        self.defaults
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    }

    /// Suite defaults overridden key by key by the case's own `deterministic`.
    #[must_use]
    pub fn deterministic(&self, case: &Case) -> Map<String, Value> {
        merged(
            self.defaults.get("deterministic"),
            case.get("deterministic"),
        )
    }

    /// `defaults.tool_input` (Tutor with tools).
    #[must_use]
    pub fn tool_input_defaults(&self) -> Map<String, Value> {
        merged(self.defaults.get("tool_input"), None)
    }
}

fn merged(base: Option<&Value>, over: Option<&Value>) -> Map<String, Value> {
    let mut out = base.and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(over) = over.and_then(Value::as_object) {
        for (k, v) in over {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Criteria of a `rubric` array. The key holding the text is `criterion` in
/// most suites and `text` in some: both are read.
#[must_use]
pub fn criteria(rubric: Option<&Value>) -> Vec<Criterion> {
    rubric.and_then(Value::as_array).map_or_default(|items| {
        items
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.to_owned();
                let text = item
                    .get("criterion")
                    .or_else(|| item.get("text"))?
                    .as_str()?
                    .to_owned();
                let weight = item.get("weight").and_then(Value::as_f64).unwrap_or(1.0);
                Some(Criterion { id, weight, text })
            })
            .collect()
    })
}

/// Every `*.cases.json` under `dir`, as `(suite id, path)`, sorted.
pub fn discover(dir: &Path) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    for role_dir in
        std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))?
    {
        let role_dir = role_dir?.path();
        if !role_dir.is_dir() {
            continue;
        }
        let role = role_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        if Role::parse(&role).is_none() {
            continue;
        }
        for file in std::fs::read_dir(&role_dir)? {
            let path = file?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if let Some(stem) = name.strip_suffix(".cases.json") {
                found.push((format!("{role}/{stem}"), path));
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The suites to run: `wanted` (ids) or all of them.
pub fn load(dir: &Path, wanted: &[String]) -> anyhow::Result<Vec<Suite>> {
    let all = discover(dir)?;
    for w in wanted {
        if !all.iter().any(|(id, _)| id == w) {
            let known: Vec<_> = all.iter().map(|(id, _)| id.as_str()).collect();
            bail!("unknown suite `{w}`; available: {}", known.join(", "));
        }
    }
    all.into_iter()
        .filter(|(id, _)| wanted.is_empty() || wanted.contains(id))
        .map(|(id, path)| {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            Suite::parse(&id, &text).with_context(|| format!("suite {id}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUITE: &str = r#"{
        "suite": "x",
        "defaults": {"runs_per_case": 3, "deterministic": {"max_words": 100, "language": "it"}},
        "cases": [
          {"id": "a", "deterministic": {"max_words": 40},
           "rubric": [{"id": "r1", "weight": 2, "criterion": "uno"}, {"id": "r2", "text": "due"}]}
        ]
    }"#;

    #[test]
    fn parses_defaults_and_case_overrides() {
        let suite = Suite::parse("tutor/conversation", SUITE).unwrap();
        assert_eq!(suite.role, Role::Tutor);
        assert_eq!(suite.runs_per_case(None), 3);
        assert_eq!(suite.runs_per_case(Some(1)), 1);
        let det = suite.deterministic(&suite.cases[0]);
        assert_eq!(det["max_words"], 40);
        assert_eq!(det["language"], "it");
    }

    #[test]
    fn a_case_declares_its_content_language_and_defaults_to_italian() {
        use domain::ContentLanguage;
        let text = |extra: &str| {
            format!(
                r#"{{"cases": [{{"id": "a"{extra}}}, {{"id": "b", "content_language": "it"}}]}}"#
            )
        };
        let suite = Suite::parse("tutor/x", &text(r#", "content_language": "en""#)).unwrap();
        assert_eq!(suite.cases[0].content_language(), ContentLanguage::En);
        assert_eq!(suite.cases[1].content_language(), ContentLanguage::It);
        let suite = Suite::parse("tutor/x", &text("")).unwrap();
        assert_eq!(suite.cases[0].content_language(), ContentLanguage::It);
        for bad in [
            r#", "content_language": "fr""#,
            r#", "content_language": "EN""#,
            r#", "content_language": 1"#,
        ] {
            assert!(Suite::parse("tutor/x", &text(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_both_spellings_of_a_criterion() {
        let suite = Suite::parse("tutor/conversation", SUITE).unwrap();
        let c = criteria(suite.cases[0].get("rubric"));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].weight, 2.0);
        assert_eq!(c[1].weight, 1.0);
        assert_eq!(c[1].text, "due");
    }

    #[test]
    fn rejects_a_suite_without_cases_or_with_an_unknown_role() {
        assert!(Suite::parse("tutor/x", r#"{"defaults": {}}"#).is_err());
        assert!(Suite::parse("nobody/x", SUITE).is_err());
    }

    #[test]
    fn a_known_check_with_a_value_of_the_wrong_type_is_refused_at_load() {
        let suite = |role: &str, det: &str| {
            Suite::parse(
                &format!("{role}/x"),
                &format!(r#"{{"cases": [{{"id": "a", "deterministic": {det}}}]}}"#),
            )
        };
        for (role, det) in [
            ("planner", r#"{"concepts_count": "4-5"}"#),
            ("planner", r#"{"concepts_max_when_general": [3]}"#),
            ("planner", r#"{"new_terms_max": {"total": 4}}"#),
            ("planner", r#"{"pitfall_form": "("}"#),
            ("generator", r#"{"prose_words": 900}"#),
            ("generator", r#"{"closing": {"words_max": "80"}}"#),
            ("generator", r#"{"sections": "3-5"}"#),
        ] {
            let err = suite(role, det).unwrap_err().to_string();
            assert!(err.contains("malformed check"), "{role} {det}: {err}");
        }
        assert!(suite("planner", r#"{"concepts_count": [4, 5]}"#).is_ok());
        assert!(
            suite(
                "generator",
                r#"{"estimated_minutes_range": {"range": [10, 15], "severity": "warn"}}"#
            )
            .is_ok()
        );
    }

    fn generator(extra_root: &str, case: &str, defaults: &str) -> anyhow::Result<Suite> {
        Suite::parse(
            "generator/x",
            &format!(
                r#"{{{extra_root} "defaults": {{{defaults}}}, "cases": [{{"id": "a" {case}}}]}}"#
            ),
        )
    }

    #[test]
    fn requirements_are_read_from_the_suite_and_the_case() {
        let suite = generator(
            r#""requires": {"web_fetch": true},"#,
            r#", "requires": {"fixture_pages": true}"#,
            "",
        )
        .unwrap();
        assert!(suite.requires.web_fetch);
        let needs = suite.requires_for(&suite.cases[0]);
        assert!(needs.web_fetch && needs.fixture_pages);
        let plain = generator("", "", "").unwrap();
        assert_eq!(plain.requires_for(&plain.cases[0]), Requires::default());

        for (root, case) in [
            (r#""requires": {"web_fech": true},"#, ""),
            (r#""requires": {"web_fetch": "yes"},"#, ""),
            (r#""requires": [],"#, ""),
            ("", r#", "requires": {"fixture_page": true}"#),
            ("", r#", "web_fetch": "on""#),
            ("", r#", "fixture_pages": {"https://a.invalid/": 1}"#),
            ("", r#", "report_metrics": ["cost"]"#),
        ] {
            assert!(generator(root, case, "").is_err(), "{root} {case}");
        }
        let twin = generator("", r#", "web_fetch": "off""#, "").unwrap();
        assert!(twin.web_fetch_off(&twin.cases[0]));
        assert!(!plain.web_fetch_off(&plain.cases[0]));
    }

    #[test]
    fn fetch_checks_are_validated_at_load_and_merged_like_the_other_checks() {
        let suite = generator(
            "",
            r#", "fetch_checks": {"fetch_errors": [1, 1]}"#,
            r#""fetch_checks": {"fetch_errors": [0, 0], "no_refetch_in_repair": true}"#,
        )
        .unwrap();
        let merged = suite.fetch_checks(&suite.cases[0]);
        assert_eq!(merged["fetch_errors"], serde_json::json!([1, 1]));
        assert_eq!(merged["no_refetch_in_repair"], true);

        for (case, defaults) in [
            (r#", "fetch_checks": {"fetch_error": [0, 0]}"#, ""),
            (r#", "fetch_checks": {"fetch_errors": 0}"#, ""),
            (r#", "fetch_checks": {"must_match_min": {"min": 1}}"#, ""),
            (
                r#", "fetch_checks": {"must_match_min": {"min": 1, "patterns": ["("]}}"#,
                "",
            ),
            ("", r#""fetch_checks": {"no_refetch_in_repair": "yes"}"#),
            ("", r#""fetch_checks": []"#),
        ] {
            assert!(generator("", case, defaults).is_err(), "{case} {defaults}");
        }
        // Only the Generator has them.
        let err = Suite::parse(
            "tutor/x",
            r#"{"cases": [{"id": "a", "fetch_checks": {"fetch_errors": [0, 0]}}]}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("only the Generator"), "{err}");
        // They may also have been moved into `deterministic`.
        assert!(generator("", "", r#""deterministic": {"web_fetch_requests": [1, 2]}"#).is_ok());
        assert!(generator("", "", r#""deterministic": {"web_fetch_requests": 1}"#).is_err());
    }

    #[test]
    fn the_shipped_suites_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
        let suites = load(&dir, &[]).unwrap();
        let ids: Vec<_> = suites.iter().map(|s| s.id.as_str()).collect();
        for expected in [
            "tutor/conversation",
            "tutor/tools",
            "tutor/starting-point",
            "researcher/sources",
            "generator/lessons",
            "generator/pdf",
            "planner/pdf",
            "planner/concepts",
            "generator/rich-lessons",
            "generator/web-fetch",
            "grader/grading",
            "recorder/records",
        ] {
            assert!(ids.contains(&expected), "{expected} missing from {ids:?}");
        }
        assert!(suites.iter().all(|s| !s.cases.is_empty()));
    }

    #[test]
    fn fixture_pages_must_be_https_and_among_the_sources() {
        let case = |url: &str, source: &str| {
            format!(r#", "sources": [{{"url": "{source}"}}], "fixture_pages": {{"{url}": "t"}}"#)
        };
        let ok = case("https://a.example/p", "https://a.example/p");
        assert!(generator("", &ok, "").is_ok());
        let http = case("http://a.example/p", "http://a.example/p");
        assert!(generator("", &http, "").is_err());
        let stray = case("https://a.example/p", "https://b.example/p");
        assert!(generator("", &stray, "").is_err());
    }
}
