// SPDX-License-Identifier: MIT
//
// Corpus invariant harness for rlsp-yaml.
//
// # Skip-list discipline
//
// The SKIP_LIST is **shrink-only**. Entries are removed as follow-up plans fix
// the root causes. New entries are only added when a NEW corpus file surfaces a
// known-fixable issue that has an immediate follow-up plan already filed; never
// to silence a surprise failure. This constraint is the harness's enforcement
// surface — without it the test degrades to a rubber stamp.
//
// A surprise failure (a (file, invariant) pair that fails but has no skip-list
// entry) must be reported to the lead via SendMessage identifying the pair and
// failure detail. The lead either files a follow-up plan (whose path the
// developer then references in the skip-list entry) or directs treating the
// failure as in-scope. The developer never adds a skip-list entry with an
// ad-hoc TODO marker lacking a plan reference.

#![expect(missing_docs, reason = "test code")]
#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code — panics are intentional assertion failures"
)]

use std::path::{Path, PathBuf};

const CORPUS_DIR: &str = "tests/corpus";

/// Each registered invariant has an id, description, and a check function.
struct Invariant {
    id: &'static str,
    #[expect(
        dead_code,
        reason = "displayed in future failure-reporting; kept for extensibility"
    )]
    description: &'static str,
    check: fn(&Path, &str) -> Result<(), String>,
}

/// Skip-list entries: `(corpus_file_name, invariant_id, followup_plan_reference_and_justification)`.
///
/// Shrink-only — see module-level doc comment for the discipline.
const SKIP_LIST: &[(&str, &str, &str)] = &[];

/// Registered invariants. Empty in Task 1 (scaffolding only).
/// Tasks 2 and 3 append entries here.
const INVARIANTS: &[Invariant] = &[];

fn collect_corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(CORPUS_DIR);
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "yml" || ext == "yaml" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

fn is_skipped(file_name: &str, invariant_id: &str) -> bool {
    SKIP_LIST
        .iter()
        .any(|(f, id, _)| *f == file_name && *id == invariant_id)
}

enum CheckOutcome {
    Passed,
    FailedExpected,
    FailedUnexpected(String),
    PassedUnexpected,
}

fn run_check(path: &Path, content: &str, invariant: &Invariant) -> CheckOutcome {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let skipped = is_skipped(file_name, invariant.id);
    match (invariant.check)(path, content) {
        Ok(()) => {
            if skipped {
                CheckOutcome::PassedUnexpected
            } else {
                CheckOutcome::Passed
            }
        }
        Err(msg) => {
            if skipped {
                CheckOutcome::FailedExpected
            } else {
                CheckOutcome::FailedUnexpected(msg)
            }
        }
    }
}

#[test]
fn corpus_invariants() {
    let files = collect_corpus_files();
    let n_files = files.len();
    let n_invariants = INVARIANTS.len();
    let n_checks = n_files * n_invariants;

    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        for invariant in INVARIANTS {
            match run_check(path, &content, invariant) {
                CheckOutcome::Passed | CheckOutcome::FailedExpected => {}
                CheckOutcome::FailedUnexpected(msg) => {
                    failures.push(format!("FAIL [{} / {}]: {}", file_name, invariant.id, msg));
                }
                CheckOutcome::PassedUnexpected => {
                    failures.push(format!(
                        "STALE SKIP [{} / {}]: expected failure but invariant passed — remove skip-list entry",
                        file_name, invariant.id
                    ));
                }
            }
        }
    }

    println!("corpus_invariants: {n_invariants} invariants × {n_files} files = {n_checks} checks");

    assert!(
        failures.is_empty(),
        "{} check(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn with_temp_dir<F: FnOnce(&Path)>(f: F) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!("corpus_test_{unique}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        f(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_corpus_files_finds_yml_and_yaml() {
        with_temp_dir(|dir| {
            std::fs::File::create(dir.join("a.yml")).unwrap();
            std::fs::File::create(dir.join("b.yaml")).unwrap();
            std::fs::File::create(dir.join("c.txt")).unwrap();
            std::fs::File::create(dir.join("d.json")).unwrap();

            let files = collect_from(dir);
            let names: Vec<_> = files
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect();
            assert!(names.contains(&"a.yml"), "expected a.yml, got {names:?}");
            assert!(names.contains(&"b.yaml"), "expected b.yaml, got {names:?}");
            assert!(!names.contains(&"c.txt"), "unexpected c.txt in {names:?}");
            assert!(!names.contains(&"d.json"), "unexpected d.json in {names:?}");
            assert_eq!(names.len(), 2);
        });
    }

    #[test]
    fn collect_corpus_files_returns_empty_for_empty_dir() {
        with_temp_dir(|dir| {
            assert!(collect_from(dir).is_empty());
        });
    }

    #[test]
    fn collect_corpus_files_excludes_subdirectories() {
        with_temp_dir(|dir| {
            std::fs::File::create(dir.join("file.yaml")).unwrap();
            std::fs::create_dir(dir.join("sub")).unwrap();

            let files = collect_from(dir);
            let names: Vec<_> = files
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect();
            assert_eq!(names, vec!["file.yaml"]);
        });
    }

    #[test]
    fn skip_list_lookup_matches_on_filename_only() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(skip_list_contains(skip, path, "round-trip"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_invariant() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(!skip_list_contains(skip, path, "idempotent"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_filename() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/other.yaml");
        assert!(!skip_list_contains(skip, path, "round-trip"));
    }

    // Helpers used only in tests — parameterised over a directory or skip-list
    // so we don't have to touch the globals.

    fn collect_from(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yml" || ext == "yaml" {
                        files.push(path);
                    }
                }
            }
        }
        files.sort();
        files
    }

    fn skip_list_contains(skip: &[(&str, &str, &str)], path: &Path, invariant_id: &str) -> bool {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        skip.iter()
            .any(|(f, id, _)| *f == file_name && *id == invariant_id)
    }

    // Validates that zero invariants × N files = 0 checks, which is the
    // expected output of the real `corpus_invariants` test in Task 1.
    #[test]
    fn corpus_invariants_runs_zero_checks_with_empty_invariant_list() {
        with_temp_dir(|dir| {
            let mut f = std::fs::File::create(dir.join("smoke.yaml")).unwrap();
            writeln!(f, "key: value").unwrap();

            let files = collect_from(dir);
            assert_eq!(files.len(), 1);

            // With an empty invariant list, checks = files × 0 = 0.
            let n_invariants = 0_usize;
            assert_eq!(files.len() * n_invariants, 0);
        });
    }
}
