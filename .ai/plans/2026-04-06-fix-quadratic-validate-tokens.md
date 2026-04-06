**Repository:** root
**Status:** InProgress
**Created:** 2026-04-06

## Goal

Fix the two highest-impact parser performance problems:
1. Eliminate O(n²) scaling in `validate_tokens()` — the
   1MB fixture takes 3.165s (19× the 100KB time for 10×
   size increase); after fix, ratio should be ≤12×.
2. Reduce heap allocations in the combinator framework —
   currently ~72 allocs per input byte (~68K allocs for a
   3KB file); target ≥50% reduction.

## Context

### O(n²) in validate_tokens()

`validate_tokens()` in `stream.rs:441-823` has three
independent quadratic patterns:

1. **Check 4 (lines 552-623):** For each byte on each
   line, `in_any(abs, &quoted)` and `in_any(abs, &flow)`
   linearly scan all ranges. O(bytes × ranges) = O(n²).

2. **Check 6 (line 684) and Check 10 (line 814):**
   `lines[..j].iter().map(|l| l.len() + 1).sum()` recomputes
   cumulative byte offsets from scratch in inner loops.

3. **Check 10 (line 809):**
   `lines[..i].iter().any(|p| ...)` rescans all preceding
   lines instead of tracking incrementally.

### Allocation budget

Investigation of the combinator framework revealed:

| Source | % of allocs | Location |
|--------|------------|----------|
| `Vec<Token>` in `Reply::Success` | 30-40% | combinator.rs:108-121 — every successful combinator match, including `satisfy()` (line 175: `Vec::new()` per character) and `opt()` failure path (line 295) |
| `Vec` growth in `many0`/`many1` | 15-20% | combinator.rs:244,266 — `Vec::new()` + repeated `extend()` |
| `String` building in events | 15-20% | event.rs:292-437 — owned `String` in public `Event` enum (API change, out of scope) |
| `Box<dyn Fn>` parser tree | 10-15% | combinator.rs:151 — rebuilt per parse (architectural, out of scope) |
| Validation Vecs | 5-10% | stream.rs:457-461 |

The highest-impact change is replacing `Vec<Token>` in
`Reply` with inline storage. Most replies carry 0-4 tokens.
`satisfy()` emits an empty Vec on every character — for a
3KB file that's thousands of empty-Vec allocations. Using
`SmallVec<[Token; 2]>` eliminates the heap allocation for
replies with ≤2 tokens (the vast majority).

The `Event` enum uses owned `String` — changing to
`&'input str` would be a public API change and is out of
scope. The `Box<dyn Fn>` parser tree rebuild is an
architectural concern deferred to the streaming tokenizer
work.

### Files involved

- `stream.rs` — validate_tokens() O(n²) fixes
- `combinator.rs` — SmallVec in Reply, capacity hints
- `Cargo.toml` — add smallvec dependency

## Steps

- [x] Investigate and confirm O(n²) root cause
- [x] Investigate allocation hot spots
- [x] Fix O(n²) patterns in validate_tokens() (39ba760)
- [x] Replace Vec<Token> with SmallVec in Reply (15f84d6)
- [ ] Run benchmarks and update documentation

## Tasks

### Task 1: Fix O(n²) patterns in validate_tokens()

Refactor `validate_tokens()` in
`rlsp-yaml-parser/src/stream.rs` to eliminate three
quadratic patterns. All changes are within this single
function.

**Changes:**

1. After collecting `quoted`, `flow`, and `block_scalars`
   ranges, verify they are sorted by start position (they
   should be since tokens are emitted in order). Add a
   debug assertion.

2. Replace the `in_any` closure (line 501-502) with a
   binary-search function:
   ```
   fn in_sorted_ranges(byte: usize, ranges: &[(usize, usize)]) -> bool
   ```
   Use `partition_point` to find the insertion point, then
   check the preceding range.

3. Pre-compute a cumulative line offset array:
   ```
   let offsets: Vec<usize> = // cumulative byte offsets
   ```
   Replace `lines[..j].iter().map(|l| l.len() + 1).sum()`
   at lines 684 and 814 with `offsets[j]`.

4. In Check 10 (lines 804-821), replace
   `lines[..i].iter().any(|p| { ... })` with an
   incremental `has_mapping` bool that updates as the
   loop progresses.

5. All existing tests must pass: `cargo test -p rlsp-yaml-parser`
6. Clippy must pass: `cargo clippy --all-targets`

- [x] Replace `in_any` with binary search
- [x] Pre-compute cumulative line offsets
- [x] Make Check 10 has_mapping incremental
- [x] All tests pass
- [x] Clippy clean

### Task 2: SmallVec in combinator Reply

Replace `Vec<Token<'i>>` with `SmallVec<[Token<'i>; 2]>`
in the `Reply` enum and all combinator functions. This
eliminates heap allocation for replies with ≤2 tokens,
which is the vast majority (satisfy, char_parser, opt
failure, single-token matches).

**Changes:**

1. Add `smallvec = "1"` to `rlsp-yaml-parser/Cargo.toml`
   dependencies.

2. In `combinator.rs`, change `Reply::Success`:
   ```rust
   use smallvec::SmallVec;
   type TokenVec<'i> = SmallVec<[Token<'i>; 2]>;

   pub enum Reply<'i> {
       Success {
           tokens: TokenVec<'i>,
           state: State<'i>,
       },
       ...
   }
   ```

3. Update all combinator functions to use `SmallVec`:
   - `satisfy()`: `SmallVec::new()` (0 tokens, inline)
   - `seq()`: `tokens_a.extend(tokens_b)` (works as-is)
   - `many0()`/`many1()`: accumulator Vec stays as
     `Vec<Token>` (these accumulate unbounded tokens),
     final return wraps in SmallVec
   - `opt()` failure: `SmallVec::new()` (0 tokens, inline)
   - `token()`: `smallvec![tok]` (1 token, inline)
   - `wrap_tokens()`: may exceed 2, will spill to heap
     (acceptable — wrapping is less frequent)

4. Update all callers that pattern-match on Reply or
   access `.tokens` — these should work via SmallVec's
   Deref<[Token]>.

5. All existing tests must pass.
6. Clippy must pass.

- [x] Add smallvec dependency
- [x] Change Reply to use SmallVec
- [x] Update all combinator functions
- [x] Update callers (stream.rs, block.rs, flow.rs, structure.rs)
- [x] All tests pass
- [x] Clippy clean

### Task 3: Run benchmarks and update documentation

Run the full benchmark suite and update
`rlsp-yaml-parser/docs/benchmarks.md` with new results.

1. Run `cargo bench -- throughput` and `cargo bench -- latency`
   and `cargo bench -- memory`
2. Verify the huge_1MB/large_100KB timing ratio is ≤12×
   for `rlsp/events`
3. Compare allocation counts before/after (memory bench)
4. Update all tables in `benchmarks.md` with new numbers
5. Update the Analysis section to reflect improvements

- [ ] Run benchmarks
- [ ] Verify scaling ratio ≤12×
- [ ] Verify allocation reduction
- [ ] Update benchmarks.md tables and analysis

## Decisions

- **Binary search over interval tree:** Binary search on
  sorted ranges is sufficient — non-overlapping, already
  sorted. An interval tree adds complexity for no benefit.
- **SmallVec<[Token; 2]> over arena:** SmallVec is a
  drop-in replacement requiring no lifetime changes. A
  token arena would eliminate more allocations but requires
  restructuring Reply to use borrowed slices — too invasive
  for this plan.
- **Event String ownership unchanged:** The public Event
  enum uses owned String. Changing to &str requires a
  public API change and lifetime threading — deferred.
- **Box<dyn Fn> parser tree unchanged:** Rebuilding the
  parser tree per parse is architectural. Addressing it
  is part of the streaming tokenizer work (item #9).
- **No advisor consultation for Tasks 1-2:** Pure
  refactoring of internal code, no behavior change, no new
  public API, no security implications. Existing
  conformance tests cover correctness.
- **Test-engineer consultation for Task 2:** SmallVec
  integration touches the core combinator framework used
  by every parser module. While it's a mechanical type
  change, the blast radius is wide — consult the TE to
  verify test coverage is adequate.
