**Repository:** root
**Status:** InProgress
**Created:** 2026-04-12

# step_in_document dispatcher micro-optimizations

## Goal

Reduce per-step overhead in `step_in_document` — the
parser's central dispatch loop — with two low-risk
micro-optimizations: (1) cache the whitespace-trim and
first-byte classification once per step instead of
repeating them across ~7 redundant probes, and (2)
short-circuit document marker checks for indented lines.
Then (3) reorder the probe cascade to try common cases
first. Each change is independently committable and
measured.

These are **diagnostic optimizations** — we expect small
wins (2–10% on `block_sequence`, 1–5% elsewhere) but the
primary goal is producing evidence about whether the probe
cascade is a measurable cost or whether the compiler
already optimizes it away (as happened with the scalar
try-chain in the prior plan's Task 3). That evidence
informs whether the full dispatcher restructure
("Option D" from the pre-plan discussion) is worth
attempting.

## Context

### Where we are today

The prior plan (`2026-04-11-parser-throughput-follow-up`)
landed three tasks:
- Task 1 (`32a2809`): unified `pos_after_line` — ~10–20%
  bare-metal wins across the board
- Task 2 (`5966502`): end-of-span char walks — ~19% on
  `flow_heavy`
- Task 3 (`8650780`): scalar try-chain first-byte dispatch
  — **no-op** (compiler had already optimized the
  speculative probes)

Bare-metal verification showed every fixture improved, no
regressions. Current ratios vs libfyaml (bare metal):

| Fixture       | ratio (rlsp/libfyaml) |
|---------------|----------------------:|
| tiny_100B     | 0.49× (rlsp wins)     |
| medium_10KB   | 1.16×                 |
| large_100KB   | 1.23×                 |
| huge_1MB      | 1.17×                 |
| block_heavy   | 1.17×                 |
| block_seq     | **1.57×** (worst)     |
| flow_heavy    | 0.70× (rlsp wins)     |
| scalar_heavy  | 1.06× (near parity)   |
| mixed         | 1.15×                 |
| k8s_3KB       | 1.29×                 |

### The probe cascade in step_in_document

`src/event_iter/step.rs:22` — 760 lines, called on every
`Iterator::next` when the parser is in `InDocument` state.
For a simple `- item` line (block sequence entry), the
function runs this probe cascade before finding the handler
at probe 13:

| # | Probe | Peek? | Trims content? |
|---|-------|-------|----------------|
| 1 | `skip_and_collect_comments_in_doc` | yes | yes (internal) |
| 2 | Tab check (line 44) | peek | no (raw `starts_with`) |
| 3 | EOF check (line 63) | `at_eof()` | no |
| 4 | `is_document_end()` (line 72) | peek | no (raw first 3 bytes) |
| 5 | `is_directives_end()` (line 91) | peek | no (raw first 3 bytes) |
| 6 | `%YAML`/`%TAG` directive (line 149) | peek + `peek_second_line()` | no (raw `starts_with`) |
| 7 | Root-node guard (line 177) | peek | no |
| 8 | Alias `*` (line 191) | peek | **trim** `' '` |
| 9 | Tag `!` (line 271) | peek | **trim** `' '` |
| 10 | Anchor `&` (line 408) | peek | **trim** `' '` |
| 11 | Flow `[`/`{` (line 574) | peek | **trim** `' '` |
| 12 | Stray `]`/`}` (line 579) | (same) | (same trimmed) |
| 13 | `peek_sequence_entry()` (line 593) | peek | **trim** `' '` |
| 14 | `peek_mapping_entry()` (line 596) | peek | **trim** `' '` |
| 15 | Dedent / close-collections (line 602) | peek | no (uses `indent`) |
| 16 | Block validity checks (line 639) | peek | no |
| 17 | Scalar try-chain (line 717) | peek | **trim** `[' ', '\t']` |

Probes 8–14 each call `content.trim_start_matches(' ')` on
the same line content. That's **7 redundant trims** — each
scanning leading whitespace from byte 0 to the first
non-space. For a 2-space-indented line, that's 7 × 2 = 14
byte comparisons. Small per-step, but the cascade runs
once per event.

Probes 4 and 5 (marker checks) run on every step but can
only match lines at column 0 (`indent == 0`). For typical
YAML where most lines are indented, these are almost always
wasted. The `Line` struct already carries `indent: usize`,
so checking `indent != 0` before calling `is_document_end`
/ `is_directives_end` is free.

### Task 3's no-op lesson

The prior plan's Task 3 showed that replacing speculative
probes with a byte-dispatch produced no measurable
throughput change — the compiler had already optimized the
linear if-else chain. The same may apply here: caching a
trim and skipping two checks may land within noise.

That's why these changes are framed as **diagnostic**: the
measurement itself is the primary deliverable. If Task 1
moves the needle, Task 2 (probe reorder) is worth doing.
If Task 1 is another no-op, Task 2 is likely also a no-op
and we stop — the compiler is already handling this,
and the full restructure (Option D) won't help either.

### Specifications and reference implementations

- [YAML 1.2 §9.1](https://yaml.org/spec/1.2.2/#91-documents)
  — document markers (`---`/`...`) must start at column 0.
  This is why the indent short-circuit in Task 1 is
  correct: any line with `indent > 0` cannot be a marker.
- [libfyaml](https://github.com/pantoniou/libfyaml) —
  performance baseline. libfyaml's scanner does exactly one
  byte-switch per token; the probe cascade is rlsp's
  highest-level dispatch overhead that libfyaml avoids.

### Acceptance criteria (applies to every task)

Same as the prior plan:

1. **Zero behaviour change.** Full test suite (including
   YAML 1.2 conformance suite, `unicode_positions.rs`, and
   `smoke.rs`) must stay green. No existing test may be
   modified.
2. **`cargo clippy --all-targets`** zero warnings (nursery
   at deny via workspace lints).
3. **`cargo fmt`** clean.
4. **Demonstrable throughput delta or no-op.** Re-run
   `cargo bench --bench throughput`, compare medians
   manually against `docs/benchmarks.md`, update the doc.
   No fixture may regress.
5. **`pub fn` inside private modules** (not `pub(crate)
   fn`) — per workspace clippy configuration.
6. **Container bench noise protocol.** If a fixture shows
   a small regression on a single run, re-run once before
   concluding it's real. ±5–12% intra-session variance on
   `block_sequence` is normal.

### Files involved

- `rlsp-yaml-parser/src/event_iter/step.rs` — the central
  dispatcher (both tasks)
- `rlsp-yaml-parser/src/lexer.rs` — `is_document_end()`,
  `is_directives_end()` (Task 1 may add indent guards
  here or in the caller)
- `rlsp-yaml-parser/docs/benchmarks.md` — updated after
  each task

## Steps

- [x] Task 1 — cache the trim + short-circuit marker
      checks (`ba11228`)
- [ ] Task 2 — reorder probes by frequency

## Tasks

### Task 1: Cache the trim + short-circuit marker checks

Two micro-optimizations in `step_in_document`:

**A. Cache the whitespace trim.** After the comment/blank
skip (line 23) and queue drain (line 31), peek the next
line once, compute the trimmed content and first byte, and
store them in local variables. Use these cached values in
probes 8–12 (alias, tag, anchor, flow, stray closer)
instead of having each probe re-trim.

Target shape (pseudocode):

```rust
// After queue drain at line 33:
let (line_ref, trimmed, first_byte) = match self.lexer.peek_next_line() {
    Some(line) => {
        let t = line.content.trim_start_matches(' ');
        let fb = t.as_bytes().first().copied();
        (Some(line), t, fb)
    }
    None => (None, "", None),
};
```

Then each probe that currently does `let trimmed =
content.trim_start_matches(' ')` uses the cached
`trimmed` and `first_byte` instead.

Note: `peek_sequence_entry()` and `peek_mapping_entry()`
are separate methods that call `peek_next_line()` and
trim internally. Do NOT change their signatures — they
are called from other contexts too. They will re-peek
(free — same cached line) and re-trim (2 byte ops on a
2-space indent), but that's only 2 redundant trims out
of the original 7. Keeping their interfaces stable is
worth the 2 extra trims.

**B. Short-circuit marker checks for indented lines.**
Before calling `is_document_end()` and
`is_directives_end()`, check whether the next line's
indent is 0. Document markers (`---`/`...`) must be at
column 0 per YAML 1.2 §9.1. Any line with `indent > 0`
cannot be a marker. The `Line` struct already carries
`indent: usize` (computed at scan time by
`lines.rs:scan_line`), so this check is:

```rust
let could_be_marker = line_ref
    .map_or(false, |l| l.indent == 0);
if could_be_marker && self.lexer.is_document_end() {
    // ...existing handler...
}
if could_be_marker && self.lexer.is_directives_end() {
    // ...existing handler...
}
```

This eliminates 2 function calls + 2 `is_marker` checks
(each ~3 byte comparisons) for every indented line.

**Implementation constraints:**

- The tab check (probe 2) operates on raw content, not
  trimmed — it checks `line.content.starts_with('\t')`.
  It runs BEFORE the cached trim and must stay before it
  because tab-indentation is detected on the raw line.
- The EOF check (probe 3) doesn't peek — it calls
  `at_eof()`. It must stay before the cached peek because
  there may be no line to peek.
- The directive check (probe 6) uses `peek_second_line()`
  — a two-line lookahead. It runs after markers and does
  not benefit from the cached trim (it operates on raw
  `starts_with("%YAML ")`). Leave it as-is.
- The root-node guard (probe 7) doesn't trim. Leave as-is.
- Borrow checker constraint: `peek_next_line()` returns
  `Option<&Line<'input>>` with lifetime tied to `self`.
  The trimmed `&str` borrows from the line's content
  (which borrows from the input). This should be fine as
  a local — the borrow lives for the duration of
  `step_in_document` and no `&mut self` call happens
  before it's consumed. But if the borrow checker
  complains, the fallback is to store `(indent, first_byte,
  leading_spaces_len)` as `Copy` values and re-slice when
  needed.

- [x] Add the cached peek + trim + first_byte at the top
  of `step_in_document` (after comment/blank skip + queue
  drain).
- [x] Replace the inline trims in alias (line ~191), tag
  (line ~271), anchor (line ~408), flow (line ~574), and
  stray closer (line ~579) probes with the cached values.
- [x] Add the `could_be_marker` indent guard before
  `is_document_end()` and `is_directives_end()`.
- [x] `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p rlsp-yaml-parser` all green.
- [x] `cargo bench --bench throughput`; compare medians
  against `docs/benchmarks.md`; update the doc.
- [x] Commit: `perf(parser): cache step_in_document
  trim and short-circuit marker checks`.

**Advisors:** test-engineer — both gates. The change is
internal-only (pure refactor of local variables + a
boolean guard on existing function calls), but it touches
the parser's hottest function. The test-engineer should
confirm that the existing test suite adequately covers the
marker-at-column-0 invariant (including `---` / `...` at
column 0 with and without leading content, and indented
lines that start with `-` or `.` which must NOT match as
markers). No security-engineer — no trust boundary change.

### Task 2: Reorder probes by frequency

Move the rarest probes after the most common ones in the
cascade. Today's order places alias (`*`), tag (`!`), and
anchor (`&`) checks before sequence/mapping detection.
In typical YAML (Kubernetes manifests, CI configs, app
config), >95% of lines are either sequence entries,
mapping entries, or scalars — aliases, tags, and anchors
are rare.

Proposed reorder (only within the "property" block — the
structural probes at the top are order-sensitive and must
not move):

**Before (current order, probes 8–14):**
1. Alias `*`
2. Tag `!`
3. Anchor `&`
4. Flow `[`/`{`
5. Stray `]`/`}`
6. Sequence entry
7. Mapping entry

**After (frequency-ordered):**
1. Sequence entry
2. Mapping entry
3. Flow `[`/`{`
4. Stray `]`/`}`
5. Alias `*`
6. Tag `!`
7. Anchor `&`

This means a `- item` line hits the sequence probe on the
first try instead of the 6th, and a `key: value` line hits
the mapping probe on the 2nd try instead of the 7th.

**Critical correctness constraint:** the reorder is only
valid if no probe's success depends on a prior probe NOT
having fired. I need to verify this holds:

- Alias, tag, and anchor probes set `pending_*` state but
  return `StepResult::Continue` (they don't emit an event
  — the next iteration handles the node they annotate).
  They would never match on the same line as a sequence
  or mapping entry because `*`, `!`, `&` are the first
  non-space character and sequence/mapping entries start
  with `-` or a plain char.
- Flow collection starts (`[`, `{`) are single-character
  indicators — they can't be confused with sequence entries
  or mapping keys.
- The dedent/close-collection block (probe 15) runs AFTER
  all probe misses and uses `line.indent` — it's
  order-independent.

The first-byte cached in Task 1 makes this analysis
trivial: if `first_byte == Some(b'-')`, only the sequence
probe can match (or a `---` marker, but that's handled
earlier). If `first_byte` is an alphabetic/digit char,
only mapping or plain scalar. The probes are
non-overlapping by first byte for all common cases.

- [ ] Move sequence entry probe (currently line ~593)
  before alias/tag/anchor/flow probes.
- [ ] Move mapping entry probe (currently line ~596)
  immediately after sequence entry.
- [ ] Move alias, tag, anchor, flow, stray-closer probes
  after mapping entry.
- [ ] Verify: no probe's match condition depends on the
  absence of a prior probe's side effect.
- [ ] `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p rlsp-yaml-parser` all green.
- [ ] `cargo bench --bench throughput`; compare medians;
  update `docs/benchmarks.md`.
- [ ] Commit: `perf(parser): reorder step_in_document
  probes by frequency`.

**Advisors:** test-engineer — both gates. The reorder
changes dispatch order for every line, which is exactly
the kind of change where the conformance suite and
`unicode_positions` tests catch regressions immediately
or not at all. The test-engineer should verify coverage
of edge cases where alias/tag/anchor appear on the same
line as structural indicators (e.g., `&anchor - item`,
`!tag key: value`, `*alias`). No security-engineer — the
change is a code-motion reorder within a single function,
no trust-boundary impact.

## Decisions

- **Do NOT change `peek_sequence_entry` /
  `peek_mapping_entry` signatures.** They are called from
  contexts other than `step_in_document` and their
  internal peek+trim is only 2 of the 7 redundant trims.
  Changing their signatures for a 2-trim savings would
  couple them to step_in_document's cache, adding
  complexity for minimal gain.
- **Do NOT restructure step_in_document into a match
  statement (Option D).** That's deferred until these
  diagnostic changes produce evidence about whether the
  probe cascade is a measurable cost. If Tasks 1 and 2
  are both no-ops, Option D will also be a no-op and we
  stop.
- **Marker short-circuit uses `indent` not first-byte.**
  Checking `indent == 0` is more precise than checking
  the first byte because `---` and `...` must be at
  column 0. A line starting with `-` at column 2 is a
  sequence entry, not a marker — using `indent == 0`
  correctly distinguishes them.
- **Reorder only the "property" probes (8–14).** The
  structural probes at the top (tab, EOF, markers,
  directives, root-node guard) are order-sensitive and
  must not move. Moving them would change error-reporting
  order on malformed input, which violates the
  zero-behaviour-change rule.
