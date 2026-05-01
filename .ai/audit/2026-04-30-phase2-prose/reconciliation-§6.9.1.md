---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: §6.9.1
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §6.9.1 Tag Resolution

A enumerated 20 requirements (4 Lenient in §6.9.1 + 1 Lenient cross-attributed to §6.8.2.2). B enumerated 23 (3 Lenient + 1 Indeterminate). Substantial cross-coverage with high agreement on the SC entries. Two distinct Lenient findings beyond Phase 1 propagations:

- **Verbatim tag admissibility** (A's REQ-3+REQ-4 / B's REQ-5): the parser accepts verbatim tags whose body neither begins with `!` nor is a valid URI. The spec's own "Invalid Verbatim Tags" example (`!<$:?>`, `!<:foo>`, `!<!>`) parses without error. Loader at `loader.rs:1010-1013` further misclassifies `!<!>` as a shorthand non-specific tag and resolves it to `!!str`, violating "verbatim tags are not subject to tag resolution."
- **Verbatim tag without separator** (A's REQ-19, A unique): `!<URI>foo` (no whitespace between `>` and content) is accepted; the verbatim path at `properties.rs:91-164` advances past `>` without an `s-separate` check. Shorthand-tag path correctly rejects the parallel case (per B's REQ-21), so the implementation is internally inconsistent.

Plus one Phase-1-propagation finding manifested at the §6.9.1 layer:

- **Post-concatenation URI validity** (B's REQ-17): even after the handle+suffix concatenation produces a tag URI, the result is not re-validated as a well-formed URI. Same root cause as Phase 1 `[93]/[94]/[95]` (prefix-scanner laxity at directive-registration time, §6.8.2.2), but B's finding is at the resolution-time check, which is scoped to §6.9.1. Distinct from A's REQ-17 cross-attribution (which targets the upstream prefix-scanner at §6.8.2.2).

And one Phase-1-propagation re-confirmed:

- **Empty shorthand suffix** (A's REQ-8 / B's REQ-6): `!!`, declared `!handle!`, and primary `!` shorthand all parse despite production [99] requiring `ns-tag-char+`. Behaviorally confirms Phase 1 `[99]` Lenient. Lives at §6.9.1.

The lead's resolution of A's `Indeterminate` (none — A had none in §6.9.1) and B's `Indeterminate` (REQ-23: unresolved tags partial representation, defers to §10): B's deferral is correct — §6.9.1 only describes that unresolved tags exist; resolution semantics are §10 schema territory and will surface in Tasks 4-6.

## Final Verdict Tally

- `Strict-conformant`: 23
- `Stricter-than-spec`: 0
- `Lenient`: 4
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 1 (cross-§10 deferral; will be re-verdicted after Tasks 4-6)
- **Total: 28 reconciled requirements**

Plus 1 cross-attribution (A's REQ-17 → §6.8.2.2 Phase 1 [93]/[94]/[95], already filed).

## Reconciled Requirement Table

| # | Topic | Final verdict | A | B | Notes |
|---|---|---|---|---|---|
| 1 | Verbatim tag delivered as-is to application | Strict-conformant | REQ-1 SC | REQ-3 SC | Both agree |
| 2 | Verbatim URI body must be `ns-uri-char+` (non-empty) | Strict-conformant | REQ-2 SC | REQ-4 SC | Both agree |
| 3 | Verbatim must begin with `!` or be valid URI (incl. `!<!>` rejection) | **Lenient** | REQ-3 + REQ-4 Lenient | REQ-5 Lenient | Both agree; spec example `!<$:?>` / `!<:foo>` / `!<!>` accepted |
| 4 | Verbatim tag must be separated from content by whitespace | **Lenient** | REQ-19 Lenient | (not tested as separate REQ) | A unique; verbatim path at properties.rs:91-164 skips the s-separate check the shorthand path enforces |
| 5 | Verbatim `%XX` decoded values not re-validated against `ns-uri-char` | Strict-conformant | REQ-18 SC | (not tested) | A unique |
| 6 | Tag and anchor properties allowed in either order | Strict-conformant | REQ-11 SC | (covered implicitly in B-1, B-2) | Both agree |
| 7 | Tag with no following content yields empty scalar | Strict-conformant | REQ-20 SC | (not tested) | A unique |
| 8 | `c-ns-tag-property` dispatches to (verbatim \| shorthand \| non-specific) | Strict-conformant | REQ-10 SC | REQ-1 + REQ-2 SC | Both agree |
| 9 | Multiple tag properties on one node = error | Strict-conformant | (not tested) | REQ-20 SC | B unique |
| 10 | Tag handle is a presentation detail; may be discarded | Strict-conformant | (not tested) | REQ-18 SC | B unique |
| 11 | Tag must be separated from content (shorthand path) | Strict-conformant | (covered in REQ-12) | REQ-21 SC | B unique; shorthand-side correct |
| 12 | Primary tag handle (`!`) defaults to `!` prefix | Strict-conformant | REQ-5 SC | REQ-7 SC | Both agree |
| 13 | Secondary tag handle (`!!`) defaults to `tag:yaml.org,2002:` | Strict-conformant | REQ-6 SC | REQ-8 SC | Both agree |
| 14 | Named tag handle (`!h!`) requires explicit `%TAG` declaration | Strict-conformant | REQ-7 SC | REQ-9 SC | Both agree |
| 15 | Empty suffix on shorthand handles is invalid (`!!`, `!handle!`) | **Lenient** | REQ-8 Lenient | REQ-6 Lenient | Both agree; Phase 1 [99] re-confirmed behaviorally |
| 16 | Shorthand suffix may not contain `!` | Strict-conformant | REQ-12 SC | REQ-13 SC | Both agree |
| 17 | Shorthand suffix may not contain `[ ] { } ,` | Strict-conformant | REQ-13 SC | REQ-14 SC | Both agree |
| 18 | Shorthand suffix `ns-tag-char` characters | Strict-conformant | (covered in REQ-12+13) | REQ-15 SC | B unique |
| 19 | Percent-encoded `%XX` sequences allowed in suffix | Strict-conformant | REQ-14 SC | REQ-16 SC | Both agree |
| 20 | Post-concatenation resolved tag must be valid URI or local tag | **Lenient** | (cross-attributed via REQ-17 to §6.8.2.2) | REQ-17 Lenient | B's distinct finding at §6.9.1 layer |
| 21 | Non-specific tag (`!`) for non-plain scalars and `?` for other nodes | Strict-conformant | REQ-9 SC | REQ-10 SC | Both agree |
| 22 | Explicit `!` non-specific tag forces failsafe resolution | Strict-conformant | (covered in REQ-9) | REQ-11 SC | B unique |
| 23 | `?` non-specific tag has no explicit syntax | Strict-conformant | (covered in REQ-9) | REQ-12 SC | B unique |
| 24 | Default tags applied by kind for untagged nodes (loader) | Strict-conformant | REQ-15 SC | (cross-§10) | A unique at §6.9.1 layer |
| 25 | `%TAG` handles scoped per document | Strict-conformant | REQ-16 SC | REQ-19 SC | Both agree |
| 26 | Tag resolution depends only on non-specific tag, path, content | Strict-conformant | (not tested) | REQ-22 SC | B unique |
| 27 | Unresolved tags allow only partial representation | Indeterminate | (not tested) | REQ-23 Indeterminate | Cross-§10 deferral |
| 28 | (Cross-attribution) `%TAG` prefix admits non-`ns-uri-char` characters | Cross-attributed | REQ-17 Lenient → §6.8.2.2 | (handled differently as REQ-17 above) | A correctly attributes upstream; matches Phase 1 [93]/[94]/[95]; already-filed |

## Resolved Defects

### Defect 1 (item 3): Verbatim tag admissibility — `!<verbatim>` body neither begins with `!` nor is required to be a valid URI

**Spec requirement (§6.9.1, "Verbatim Tags"):** "Verbatim tags must either begin with a `!` (a local tag) or be a valid URI (a global tag)." Spec Example 6.25 ("Invalid Verbatim Tags") explicitly lists `!<!>`, `!<$:?>`, and `!<:foo>` as ERRORS.

**Implementation gap.** `properties.rs:91-164` validates the verbatim body against `ns-uri-char+` only — character class membership. The higher-level prose constraint ("must begin with `!` or be a valid URI") is not enforced. Loader at `loader.rs:1010-1013` further has a bare-`!` shortcut that misclassifies verbatim `!<!>` as a shorthand non-specific tag and resolves it to `!!str`, conflating verbatim and shorthand sources.

**Behavioral evidence (both auditors):**

- A's REQ-3: input `!<$:?> foo` parses to `Scalar(tag="$:?", value="foo")` — the body is not a valid URI but is accepted.
- A's REQ-4: input `!<!> foo` parses to `Scalar(tag="!!str", value="foo")` — the verbatim body is `!`, which the spec calls invalid; the loader's bare-`!` shortcut additionally translates it to `!!str`.
- B's REQ-5: same observations under different test inputs; confirms the misclassification path through the loader.

**Lead verdict:** Lenient. The character-class check is correct (REQ-2 / item 2 SC) but the prose-level admissibility rule is unenforced. The loader's bare-`!` shortcut is a downstream conflation, not a separate defect — fixing the verbatim admissibility check at the `properties.rs` layer would stop `!<!>` from reaching the loader's shortcut.

**Fix sketch:** in the verbatim arm at `properties.rs:91-164`, after extracting the URI body, additionally check whether the body starts with `!` (local tag form) OR matches a URI well-formedness predicate (global tag form). The spec doesn't define a precise URI predicate at the BNF level, but RFC 3986 absolute-URI form is the conventional reading. A pragmatic check: reject bare `!` alone; reject bodies starting with characters that cannot start a URI scheme or local tag.

### Defect 2 (item 4): Verbatim tag without separator before node content

**Spec requirement (§6.9, §6.9.1, and §6.7 block-node grammar):** node properties must be followed by `s-separate(n,c)` before content.

**Implementation gap.** The verbatim arm at `properties.rs:91-164` advances by `1 (`<`) + uri.len() + 1 (`>`)` and returns. The caller treats the position after `>` as the start of node content; no `s-separate` check is enforced for verbatim tags. The shorthand arm correctly emits "tag must be separated from node content by whitespace" (per `step.rs:502-516`), so the implementation is internally inconsistent — verbatim is laxer than shorthand.

**Behavioral evidence (A's REQ-19):** input `!<tag:yaml.org,2002:str>foo` parses to `Scalar(tag="tag:yaml.org,2002:str", value="foo")` — accepted with no whitespace between `>` and `foo`. The parallel shorthand input `!!str foo` (without space) would be rejected.

**Lead verdict:** Lenient. B did not test this case and verdicted shorthand path correctly as SC (REQ-21); A's verbatim case adds the asymmetry finding that B missed.

**Fix sketch:** after the verbatim closing `>`, the caller must enforce `s-separate(n,c)` before processing content. Mirror the shorthand path's check at `step.rs:502-516`.

### Defect 3 (item 15): Empty shorthand suffix accepted

**Spec requirement (§6.9.1, production [99] `c-ns-shorthand-tag`):** `c-tag-handle ns-tag-char+` — non-empty suffix required.

**Implementation gap (Phase 1 [99] propagation).** `event_iter/properties.rs:170-176` and `:200-203` explicitly accept empty suffixes for primary and named handles. Comments in code document this acceptance.

**Behavioral evidence (both auditors):**

- A's REQ-8: `%TAG !e! tag:example.org,2026:!\n---\n!e! foo\n` parses to `Scalar(tag="tag:example.org,2026:!", value="foo")` — empty suffix accepted.
- B's REQ-6: same observation across `!!`, declared `!h!`, and `!`-handle variants.

**Lead verdict:** Lenient. This is the Phase 1 `[99]` finding re-confirmed at the behavioral level; lives at §6.9.1.

**Fix sketch:** in the shorthand arm at `properties.rs:170-216`, after the second `!` is consumed, require at least one `ns-tag-char` before whitespace/EOL. Phase 1 [99] follow-up is already filed (commit `775020c`); this audit confirms behavior matches the BNF-level finding.

### Defect 4 (item 20): Post-concatenation tag URI not re-validated

**Spec requirement (§6.9.1, "Tag Shorthands"):** the resolved tag (handle prefix + suffix concatenation) is the final tag URI; it must be a valid URI or local tag.

**Implementation gap.** Phase 1 `[93]/[94]/[95]` Lenient at the `%TAG` directive layer (§6.8.2.2) means the parser accepts prefix bytes outside the `ns-uri-char` set. The directive-time scan accepts those bytes; the resolution-time concatenation produces a tag URI that may contain spaces, `{`, `!`, etc. The §6.9.1 layer never re-checks the concatenation result.

**Behavioral evidence (B's REQ-17):** `%TAG !x! tag:!badprefix!\n---\n!x!suffix foo\n` produces a resolved tag string containing the bad prefix bytes; the loader carries it through to the AST node without complaint.

**Lead verdict:** Lenient. A correctly cross-attributed the directive-time scanner laxity to §6.8.2.2 (which is Phase 1's finding); B finds the §6.9.1 manifestation at the resolution-time check. Both views are valid; the §6.9.1 view captures the resolution-time rule that the implementation skips.

**Fix sketch:** after handle+suffix concatenation in the resolution path, re-validate the result against `ns-uri-char+`. Alternatively, fixing the directive-time scanner (Phase 1 [93]/[94]/[95] follow-up) propagates here automatically.

## Architectural Findings (cross-cutting)

### A1. Loader's bare-`!` shortcut conflates verbatim and shorthand sources

**Observation (B's architectural note):** `loader.rs:1010-1013` has a shortcut where any tag value of `!` (regardless of whether it arrived from verbatim `!<!>` or shorthand `!`) is treated as the non-specific `!` tag and resolved per kind. This conflation is what causes verbatim `!<!>` to resolve to `!!str` rather than be rejected as an invalid verbatim body.

**Disposition:** the proper fix is at the verbatim admissibility check (Defect 1), not the loader. Once `!<!>` is rejected at `properties.rs`, the loader shortcut never sees it.

### A2. Verbatim/shorthand asymmetry (separator enforcement)

**Observation (A's REQ-19 finding):** verbatim tags don't require `s-separate` before content; shorthand tags do. The asymmetry is a direct consequence of the verbatim arm at `properties.rs:91-164` returning immediately after `>` without re-entering the property-completion path that enforces s-separate.

**Disposition:** structural fix per Defect 2.

## Disposition for Phase 2 Summary (Task 8)

**New follow-up entries to file:**

1. **Verbatim tag admissibility — body must begin with `!` or be valid URI (§6.9.1, item 3)** — `properties.rs:91-164` validates only `ns-uri-char+`; the prose-level admissibility rule is unenforced. Spec Example 6.25 cases (`!<!>`, `!<$:?>`, `!<:foo>`) all parse without error. Loader's bare-`!` shortcut at `loader.rs:1010-1013` further conflates verbatim `!<!>` with shorthand non-specific `!`. Verdict `Lenient`.

2. **Verbatim tag separator enforcement (§6.9.1, item 4)** — `properties.rs:91-164` does not require whitespace between the verbatim closing `>` and node content. Shorthand path enforces this correctly at `step.rs:502-516`. Inconsistent. Verdict `Lenient`.

3. **Post-concatenation tag URI validity (§6.9.1, item 20)** — handle+suffix concatenation result is not re-validated as a valid URI. Same observable behavior as Phase 1 [93]/[94]/[95] but the §6.9.1 layer's rule (resolved tag must be valid URI) is independent of the §6.8.2.2 prefix-scan layer. Verdict `Lenient`. Could be deduplicated with the Phase 1 follow-up at the user's discretion (single-fix-resolves-both pattern), or filed separately if the user wants the §6.9.1-specific check.

**Items deduplicated against existing follow-ups** (cited in summary's Deduplicated subsection):

- **Empty shorthand suffix (item 15)** — Phase 1 [99] follow-up already filed at commit `775020c` ("Empty suffix in shorthand tags"). Same spec section + same code location → dedup.

**Items deferred to §10 schema audit (Tasks 4-6):**

- **Item 27 (unresolved tags allow partial representation)** — Indeterminate at §6.9.1; B correctly defers to §10 schema-resolution audit, where strict-mode JSON schema rejection paths are tested.

## Methodology Notes

- The dual-track methodology surfaced complementary findings: A caught the verbatim-no-separator asymmetry that B missed; B caught the post-concatenation URI-validity gap that A cross-attributed (correctly per the symmetric reconciliation principle, but as a result A didn't have a §6.9.1-layer Lenient for it). Both views are valid and the lead retains both, with A's view as the cross-attribution and B's view as the §6.9.1-scoped Lenient.
- Both auditors applied "should is non-mandatory" correctly — neither verdicted any §6.9.1 case as Lenient on a "should warn" basis. The architectural Warning-channel observation from §6.8 didn't surface new cases here.
- Probe cleanup held: both used `/tmp/audit-probe-§6.9.1-{a,b}/` outside the git tree; final `git status` clean.
- B's Indeterminate at REQ-23 is correctly scoped (defer to §10). The lead does not need to investigate further at the §6.9.1 layer; the §10 audit will cover the resolution semantics.
