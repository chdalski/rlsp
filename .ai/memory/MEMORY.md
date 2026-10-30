# Memory Index

## Feedback — how to work

- [Present full plan before approval](feedback_plan_presentation.md) — show complete plan content, not a summary, before asking for approval
- [One plan at a time](feedback_one_plan_at_a_time.md) — present and execute plans sequentially; don't batch-present multi-plan programs
- [No pausing between tasks](feedback_no_pausing_between_tasks.md) — after plan approval, run all tasks + push + CI + closure continuously; report once at the end or on a real blocker
- [No non-decision questions](feedback_no_non_decision_questions.md) — don't ask the user to pick between options that all lead to an acceptable outcome; resolve low-stakes non-blocking items as lead
- [Don't auto-proceed on question timeouts](feedback_no_auto_proceed_on_question_timeout.md) — on AskUserQuestion "no response after 60s" notices, hold and wait; don't guess user-owned decisions
- [Trunk-based: land on main](feedback_trunk_based_land_on_main.md) — land work directly on main, no feature-branch/PR ceremony; let Dependabot self-close superseded PRs (`@dependabot rebase` only if still open)
- [Verify CI pipeline after push](feedback_verify_ci_pipeline_after_push.md) — check `gh run list` (full matrix, incl. Windows) after pushing; local gates aren't "CI green"; run `tsc --noEmit` as part of the TS gate
- [CI wait loops need the full SHA](feedback_ci_wait_loop_full_sha.md) — `gh run list --commit <short-sha>` returns nothing; use `git rev-parse` and sanity-check the query before backgrounding a wait
- [yaml-test-suite is mandatory coverage](feedback_yaml_test_suite_mandatory.md) — any invariant/property testable against the suite MUST be tested against it; synthetic fixtures are never a substitute
- [Always consider performance](feedback_always_consider_performance.md) — proactively assess perf implications for every parser fix; don't wait to be asked
- [No agent-verifiable perf thresholds](feedback_no_agent_perf_thresholds.md) — agents run in Docker, perf targets are baremetal; perf measurement is the user's job out-of-band
- [No mechanical code splits](feedback_no_mechanical_splits.md) — line counts are diagnostic signals, not prescriptive rules; splits need structural justification
- [Scratch tools are not deliverables](feedback_scratch_tools_not_deliverables.md) — when a task requires empirical measurement, call out in the dispatch that scratch files must be deleted before committing
- [Verify capabilities before claiming gaps](feedback_verify_before_claiming_gaps.md) — check `server.rs` `capabilities()` and handlers before listing LSP features as missing
- [No completed items in memory](feedback_no_completed_in_memory.md) — remove done items from follow-up queues; completed work lives in plans and git
- [No edits to completed plans](feedback_no_edit_completed_plans.md) — completed plans are immutable history; don't update stale refs in them
- [Project memory location](feedback_project_memory_location.md) — use `.ai/memory/` in the workspace for project-specific memory, not home-dir

## Project — open work and non-obvious facts

- [Follow-up queue](project_followup_plans.md) — open work: plain-scalar comment folding plan, then comment preservation plan, then flow-sequence comment plan, then alias-expansion depth, then discuss 3c; rlsp-yaml follow-ups, WORKLIST→`#[ignore]`, parser bindings, CI items
- [Deferred perf candidate + methodology](potential-performance-optimizations.md) — arena allocation for the Event queue (no profile evidence yet); baremetal flamegraph/bench/LLVM-IR methodology before any perf plan
- [rust-toolchain.toml pins CI too](project_rust_toolchain_pin_ci_behavior.md) — the root pin overrides dtolnay's rustup default; `stable` and the pin are separate installs; bump pin channel + every dtolnay ref together; Zed crate stays on Zed's registry toolchain (1.90)
- [Plugin update needs version bump](project_claude_plugin_update_needs_version_bump.md) — version resolution order; removing `version` or a plugin-root CLAUDE.md fails `validate --strict`
- [crates.io OIDC token is publish-only](project_crates_io_oidc_scope.md) — in-session `CARGO_REGISTRY_TOKEN` is OIDC-scoped to publish; `cargo yank` returns 403 and requires a personal token
- [Dependabot npm runs fail on its own 3-day cooldown](project_dependabot_npm_cooldown.md) — `ERR_PNPM_NO_MATURE_MATCHING_VERSION` comes from Dependabot's injected `minimumReleaseAge=4320`, not repo config; self-heals after 72 h
- [Dependabot group change skips excluded dep once](project_dependabot_group_change_skips_excluded_dep.md) — push-triggered run rebuilds the open group PR but gives the newly excluded dep no PR; next weekly run or a manual "Check for updates" (web UI only) opens it
