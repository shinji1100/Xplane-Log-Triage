use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ExpectedCase {
    description: String,
    must_detect: Vec<ExpectedFinding>,
    must_not_detect: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedFinding {
    kind: String,
    min_occurrences: usize,
}

#[test]
fn real_log_corpus_matches_expected_findings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("real");
    let cases = case_dirs(&root);
    assert!(
        !cases.is_empty(),
        "No corpus cases found under {}",
        root.display()
    );

    for case_dir in cases {
        run_case(&case_dir);
    }
}

fn case_dirs(root: &Path) -> Vec<PathBuf> {
    let mut cases = Vec::new();
    if !root.exists() {
        return cases;
    }

    for entry in fs::read_dir(root).expect("read samples/real directory") {
        let entry = entry.expect("read corpus directory entry");
        let path = entry.path();
        if path.is_dir() {
            cases.push(path);
        }
    }

    cases.sort();
    cases
}

fn run_case(case_dir: &Path) {
    let case_id = case_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>");
    let log_path = case_dir.join("Log.txt");
    let expected_path = case_dir.join("expected.json");

    let log = fs::read_to_string(&log_path)
        .unwrap_or_else(|err| panic!("case {case_id}: failed to read Log.txt: {err}"));
    let expected_text = fs::read_to_string(&expected_path)
        .unwrap_or_else(|err| panic!("case {case_id}: failed to read expected.json: {err}"));
    let expected: ExpectedCase = serde_json::from_str(&expected_text)
        .unwrap_or_else(|err| panic!("case {case_id}: invalid expected.json: {err}"));

    let findings = xplane_doctor::analyze_log(&log);

    for required in &expected.must_detect {
        let finding = findings
            .iter()
            .find(|finding| finding.kind == required.kind)
            .unwrap_or_else(|| {
                panic!(
                    "case {case_id} ({desc}): missing expected kind `{kind}`; detected kinds: {detected}",
                    desc = expected.description,
                    kind = required.kind,
                    detected = detected_kinds(&findings)
                )
            });

        assert!(
            finding.occurrences >= required.min_occurrences,
            "case {case_id} ({desc}): kind `{kind}` occurrences {actual} < expected {expected_min}",
            desc = expected.description,
            kind = required.kind,
            actual = finding.occurrences,
            expected_min = required.min_occurrences
        );
    }

    for forbidden_kind in &expected.must_not_detect {
        assert!(
            !findings
                .iter()
                .any(|finding| &finding.kind == forbidden_kind),
            "case {case_id} ({desc}): unexpected forbidden kind `{kind}` was detected",
            desc = expected.description,
            kind = forbidden_kind
        );
    }
}

fn detected_kinds(findings: &[xplane_doctor::Finding]) -> String {
    if findings.is_empty() {
        return "<none>".to_string();
    }

    findings
        .iter()
        .map(|finding| format!("{}({})", finding.kind, finding.occurrences))
        .collect::<Vec<_>>()
        .join(", ")
}
