---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: §10.1
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §10.1 Failsafe Schema

Both auditors enumerated 8 normative requirements and verdicted all 8 as `Strict-conformant`. Numbering differs slightly (A's REQ-6 is "Failsafe selectable" / B's REQ-6 is "`!` resolves by kind"; both cover the full 8-requirement scope). No disagreements; cleanest reconciliation of Phase 2 so far.

## Final Verdict Tally

- `Strict-conformant`: 8
- `Stricter-than-spec`: 0
- `Lenient`: 0
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0
- **Total: 8 reconciled requirements**

## Agreed Requirements (8 entries, by subject matter)

| Topic | Final verdict | A entry | B entry |
|---|---|---|---|
| Failsafe defines exactly three tags (`!!str`, `!!seq`, `!!map`) | Strict-conformant | REQ-1 | REQ-1 |
| All scalars resolve to `!!str` regardless of content | Strict-conformant | REQ-2 | REQ-2 |
| All sequences resolve to `!!seq` | Strict-conformant | REQ-3 | REQ-3 |
| All mappings resolve to `!!map` | Strict-conformant | REQ-4 | REQ-4 |
| Plain and quoted scalars resolve identically (both → `!!str`) | Strict-conformant | REQ-5 | REQ-5 |
| `!` non-specific tag resolves by kind under Failsafe | Strict-conformant | REQ-7 | REQ-6 |
| Explicit non-failsafe tags pass through unmodified | Strict-conformant | REQ-8 | REQ-7 |
| Schema selection is per-loader; Failsafe is selectable | Strict-conformant | REQ-6 | REQ-8 |

## Resolved Disagreements

None.

## Implementation Highlights (cross-cutting evidence both auditors converged on)

- **`resolve_scalar` Failsafe arm is structurally constant** at `schema.rs:130-131` — never inspects style or content; matches the §10.1 guarantee that all scalars resolve to `!!str`.
- **`resolve_collection` discards its `schema` parameter** (`let _ = schema` at `schema.rs:178`) — all three schemas share the kind-only collection rule, which matches §10.1 (and §10.2 / §10.3 do not differ on collection resolution).
- **Bare `!` on collections is correctly normalised** via `effective_tag = tag.filter(|t| *t != "!")` at `loader.rs:1038/1052`, then resolved by kind. Confirms §10.1's "`!` non-specific resolves by kind" requirement.

## Architectural Findings (cross-cutting, both auditors recorded)

### A1. `?` non-specific status not preserved as "unresolved"

**Observation (A):** the parser does not preserve "left unresolved" status for `?`-tagged nodes; under Failsafe they resolve to `!!str`/`!!seq`/`!!map` immediately rather than carrying an "unresolved" marker. Spec §6.9.1 / §3.3.2 mention partial representation as permissible when nodes remain unresolved, but Failsafe's resolution rules apply to all nodes by kind, so universal resolution is consistent with §10.1.

**Disposition:** matches universal Failsafe ecosystem practice (libyaml, PyYAML, snakeyaml all do the same). Not a defect. Recorded for downstream-design awareness only.

### A2. Default schema is `Core`, not `Failsafe`

**Observation:** the loader's default schema is `Core` (per `schema.rs` definitions, consistent with §10.3 spec line 6612 which calls Core "recommended default"). Failsafe is selectable via `LoaderBuilder::schema(Schema::Failsafe)`. Both auditors confirmed selectability.

**Disposition:** spec-consistent. The Core default matches §10.3's recommendation; Failsafe is correctly available as an alternative.

### A3. Explicit foreign tags pass through under Failsafe

**Observation (B):** explicit non-Failsafe tags like `!!int`, `!!bool`, or local `!foo` pass through the AST under Failsafe with no diagnostic. §10.1 governs **resolution** of unresolved nodes, not source-side rejection of tags from other schemas. The Failsafe schema doesn't define `!!int` etc., but the spec doesn't require Failsafe to reject them either — the source-tag preservation is correct.

**Disposition:** spec-consistent. Foreign tags are preserved as-is per §6.9.1's "tags delivered to application as-is" principle; the application decides what to do with `!!int` under a Failsafe-loading consumer.

## Disposition for Phase 2 Summary (Task 8)

No follow-up entries to file from §10.1. All 8 requirements are conformant. The architectural findings are informational and don't generate fix-tracking entries.

## Methodology Notes

- §10.1 is the simplest schema and the cleanest reconciliation (8/8 SC, zero disagreements). Both auditors converged on the same structural insight: the parser implements Failsafe via `resolve_scalar`'s constant arm + `resolve_collection`'s schema-discarding kind-based dispatch, which is exactly what §10.1 requires.
- Probe cleanup held: both auditors used `/tmp/audit-probe-§10.1-{a,b}/` outside the git tree; final `git status` clean.
- The simplicity of §10.1 (compared to §10.2 JSON and §10.3 Core) means §10.2 and §10.3 audits will be substantially more complex. The next two tasks should expect more requirements per area and possibly Lenient findings on edge-case regex coverage.
