# Memory Index

- [Clean-build before lint verification](feedback_clean_build_before_lint_verification.md) — after a toolchain change, `cargo clean` before trusting clippy; incremental cache hid 92 findings during the 1.97 adoption
- [yaml-test-suite is mandatory coverage](feedback_yaml_test_suite_mandatory.md) — any invariant/property testable against the suite MUST be tested against it; synthetic fixtures are never a substitute
- [crates.io OIDC token is publish-only](project_crates_io_oidc_scope.md) — in-session `CARGO_REGISTRY_TOKEN` is OIDC-scoped to publish; `cargo yank` returns 403 and requires a personal token
- [project_followup_plans.md](project_followup_plans.md) — Open items: feature work (#1-3), cleanup queue (C1-C4: stale refs, match refactors, iterator patterns)
- [potential-performance-optimizations.md](potential-performance-optimizations.md) — Deferred perf candidates: Option D (step_in_document restructure), L4 full (Option<Box<NodeMeta>>), arena Event queue, lazy Span construction + verification methodology. Applied work lives in plans + git log.
- [2026-04-18-rlsp-yaml-architectural-program.md](2026-04-18-rlsp-yaml-architectural-program.md) — Session brief: GHA-expression false-positive bug → 3-move architectural program (AST-first rule, LSP-feature fixtures, real-world corpus). Move 1 plan drafted at `.ai/plans/2026-04-18-one-parser-one-ast.md`, awaiting user approval.
