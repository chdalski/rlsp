---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

## Completed

1. ~~Replace saphyr in rlsp-yaml-parser test infrastructure~~ DONE
2. ~~Write rlsp-yaml-parser/README.md~~ DONE
3. ~~Wire up contentSchema validation~~ DONE
4. ~~Add libfyaml's custom test cases~~ DONE
5. ~~Remove saphyr workarounds~~ DONE (2026-04-06)
6. ~~Benchmark comparison~~ DONE (2026-04-06) — results in `rlsp-yaml-parser/docs/benchmarks.md`
7. ~~Fix O(n²) scaling + SmallVec allocation reduction~~ DONE (2026-04-06) — scaling ratio 19× → 10.1×, allocations reduced 13.7% via SmallVec

## Open — Performance Optimization

8. **Streaming tokenizer + allocation reduction** — Two goals, one architectural change: (a) O(1) first-event latency (currently O(n) — buffers entire input), (b) reduce allocations by replacing Box<dyn Fn> combinator framework with a state machine (eliminates 30-40% of allocs from boxed closures, 15-20% from Reply Vecs, enables Event to borrow from input instead of owning Strings). Biggest remaining change. **In progress as of 2026-04-07** — plan `.ai/plans/2026-04-07-streaming-parser-rewrite.md`, Tasks 1-8 committed.

## Open — Post-streaming-rewrite cleanup

9. **Promote `clippy::panic` from crate-level to workspace-level** — Task 9 of the streaming rewrite adds `#![deny(clippy::panic)]` at the top of `rlsp-yaml-parser-temp/src/lib.rs` (crate-scoped, stacks additively with workspace inheritance). After Task 23 migration ships, this crate-level attribute should be removed and the lint promoted to workspace-wide. The skill template `.claude/skills/project-init/rust-init.md` already includes `panic = "deny"` in both its rustfmt and lint-inheritance blocks, so the procedure is: (a) run `/project-init` to regenerate workspace `Cargo.toml` from the template, which adds `panic = "deny"` to `[workspace.lints.clippy]`; (b) remove `#![deny(clippy::panic)]` from the top of `rlsp-yaml-parser/src/lib.rs` (post-migration, crate was renamed from `-temp`); (c) verify `cargo clippy --workspace --all-targets` stays clean for all crates; (d) fix any new violations in `rlsp-fmt`, `rlsp-yaml`, or elsewhere that the workspace lint now catches. This makes the panic ban global and prevents regression in any future crate.

## Open — Feature Work (lower priority)

11. **YAML version selection** — `yaml.yamlVersion` for 1.1 vs 1.2 boolean interpretation (`on`/`off`/`yes`/`no`)
12. **Flow style enforcement levels** — RedHat can forbid flow style (ERROR), we only warn. Add a severity setting on existing flowMap/flowSeq diagnostics.
13. **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.
