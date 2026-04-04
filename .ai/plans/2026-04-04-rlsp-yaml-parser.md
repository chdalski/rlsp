**Repository:** root
**Status:** InProgress
**Created:** 2026-04-04

## Goal

Build `rlsp-yaml-parser`, a full-stack YAML 1.2 parser crate
that replaces saphyr with a spec-faithful, fully conformant
implementation. The parser must pass 100% of the YAML test
suite, preserve comments and spans as first-class data, and
match or approach libfyaml's throughput. This gives the RLSP
project a parser without the workarounds saphyr forces
(discarded comments, zero container spans, eager alias
resolution, lost chomping indicators, silent key
deduplication) and positions `rlsp-yaml-parser` as a
high-quality crates.io package.

## Context

- **Saphyr limitations driving this work:** 6 documented
  workarounds in rlsp-yaml (formatter comment
  extract/reattach, recursive span computation for
  containers, text-based duplicate key detection, etc.).
  Saphyr scores 89.6% valid / 70.2% invalid on the YAML
  test matrix — not conformant enough for a language server.
- **Architecture decision:** implement from the YAML 1.2
  spec's 211 formal grammar productions, not as a libyaml
  port. HsYAML (GPL-2.0, cannot translate) demonstrates
  this approach works — 97.1% valid / 100% invalid with
  ~5,600 lines of Haskell. We use the spec as primary
  source; HsYAML only as reference for resolving spec
  ambiguities.
- **Crate structure:** new workspace member at
  `rlsp-yaml-parser/`, publishable independently to
  crates.io. Full stack: combinator framework → tokens →
  events → AST → schema resolution → emitter.
- **Benchmark baseline:** libfyaml (C, 100%/100%
  conformant) installed in devcontainer. Comparison
  benchmarks are temporary — dropped once our own
  performance baselines are established.
- **Migration strategy:** build complete, then swap into
  rlsp-yaml in a separate plan. No phased integration.
- **Workspace conventions:** edition 2024, MSRV 1.87,
  workspace lint inheritance (clippy pedantic+nursery at
  warn, `unwrap_used`/`expect_used`/`indexing_slicing` at
  deny, `warnings = "deny"`).
- **Key files:**
  - `/workspace/Cargo.toml` — workspace root
  - `/workspace/rlsp-yaml/src/parser.rs` — current saphyr
    wrapper (replacement target)
  - `/workspace/rlsp-yaml/tests/conformance.rs` — existing
    conformance test infrastructure
  - `/workspace/rlsp-yaml/tests/yaml-test-suite/` — vendored
    test suite (351 test files)

## Steps

- [x] Create crate scaffold, position/span types, and
      parser combinator framework (6b1d449)
- [x] Implement character productions (spec §5) and
      encoding detection (cbaa4c2)
- [ ] Implement structural productions (spec §6) —
      indentation, comments, separation, directives, node
      properties
- [ ] Implement flow style productions (spec §7) — plain
      scalars, quoted scalars, flow sequences, flow mappings
- [ ] Implement block style productions (spec §8) — block
      scalars (literal/folded with chomping), block
      sequences, block mappings
- [ ] Implement document stream productions (spec §9) —
      document boundaries, bare/explicit documents,
      multi-document streams
- [ ] Build event layer — token-to-event conversion with
      public streaming API
- [ ] Build AST loader — events to node graph with
      anchor/alias resolution and cycle detection
- [ ] Implement schema resolution — failsafe/JSON/core
      schemas, tag resolution, scalar type inference
- [ ] Implement emitter — node-to-YAML serialization with
      style and comment preservation
- [ ] Integrate YAML test suite and reach 100% conformance
- [ ] Add benchmarks with libfyaml comparison baseline

## Tasks

### Task 1: Crate scaffold and combinator framework

Set up `rlsp-yaml-parser/` as a workspace member and build
the parser combinator framework that all 211 productions
will use.

**Files:** `rlsp-yaml-parser/Cargo.toml`, `src/lib.rs`,
`src/pos.rs`, `src/token.rs`, `src/combinator.rs`

- [x] Create `rlsp-yaml-parser/` with Cargo.toml inheriting
      workspace lints, edition 2024, MSRV 1.87
- [x] Add to workspace members in root `Cargo.toml`
- [x] Define `Pos` type (byte offset, char offset, line,
      column) and `Span` type (start + end pos)
- [x] Define `Token` type with `Code` enum (~40 variants:
      `BeginMapping`, `EndMapping`, `BeginScalar`,
      `EndScalar`, `Text`, `Indicator`, `BeginComment`,
      `EndComment`, `BeginAnchor`, `EndAnchor`,
      `BeginAlias`, `EndAlias`, `BeginTag`, `EndTag`,
      `BeginDocument`, `EndDocument`, `BeginSequence`,
      `EndSequence`, `DirectivesEnd`, `DocumentEnd`,
      `BeginNode`, `EndNode`, etc.). Every token carries a
      `Pos`.
- [x] Build parser combinator framework:
      - `Parser` type: `State → Reply` (state = input chars
        + position + context)
      - Core combinators: sequence (`&`), alternative (`/`),
        repetition (`*`, `+`, `?`), exclusion (`-`),
        lookahead (positive/negative), commitment/cut
      - Context threading: indentation level `n`, context
        mode `c` (BlockOut, BlockIn, FlowOut, FlowIn,
        BlockKey, FlowKey)
      - Token emission: `wrap_tokens(begin, end, parser)`
        for Begin/End pairs
      - Position tracking: automatic pos stamping on tokens
      - Error reporting: position + message, with recovery
        hints
- [x] Unit tests for every combinator (sequence, choice,
      repetition, lookahead, commit, indentation threading)

### Task 2: Character productions and encoding (§5)

Implement the 62 character productions from YAML 1.2 spec
§5 and UTF-8/16/32 encoding detection.

**Files:** `src/encoding.rs`, `src/chars.rs`

- [x] BOM detection and encoding selection (UTF-8, UTF-16
      LE/BE, UTF-32 LE/BE)
- [x] Character classification productions [1]–[62]:
      `c_printable`, `nb_char`, `b_char`, `b_line_feed`,
      `b_carriage_return`, `b_break`, `b_as_line_feed`,
      `b_non_content`, `s_space`, `s_tab`, `s_white`,
      `ns_char`, `nb_json`, `c_indicator` (21 indicators),
      `c_flow_indicator`, `ns_plain_safe(c)`,
      `ns_plain_first(c)`, `ns_plain_char(c)`,
      `ns_uri_char`, `ns_tag_char`, `c_escape`,
      `ns_esc_*` (15 escape variants), `c_ns_esc_char`
- [x] Line break normalization (CRLF → LF)
- [x] Unit tests for each character class, especially edge
      cases: BOM handling, surrogate pairs, escape sequences

### Task 3: Structural productions (§6)

Implement the 41 structural productions from spec §6 —
the backbone that handles indentation, comments, whitespace,
directives, and node properties.

**Files:** `src/structure.rs` (or integrated into scanner)

- [ ] Indentation: `s_indent(n)`, `s_indent_lt(n)`,
      `s_indent_le(n)` — exact and bounded indentation
      matching
- [ ] Separation spaces: `s_separate_in_line`,
      `s_block_line_prefix(n)`, `s_flow_line_prefix(n)`,
      `s_line_prefix(n,c)`, `l_empty(n,c)`,
      `b_l_trimmed(n,c)`, `b_as_space`, `b_l_folded(n,c)`,
      `s_flow_folded(n)`, `s_separate(n,c)`,
      `s_separate_lines(n)`
- [ ] Comments: `c_nb_comment_text`, `b_comment`,
      `s_b_comment`, `l_comment`, `s_l_comments` —
      comments wrapped in `BeginComment`/`EndComment` token
      pairs (first-class, not discarded)
- [ ] Directives: `l_directive`, `ns_yaml_directive`,
      `ns_yaml_version`, `ns_tag_directive`,
      `c_tag_handle`, `c_primary_tag_handle`,
      `c_secondary_tag_handle`, `c_named_tag_handle`,
      `ns_tag_prefix`, `c_ns_local_tag_prefix`,
      `ns_global_tag_prefix`, `ns_reserved_directive`
- [ ] Node properties: `c_ns_properties(n,c)`,
      `c_ns_tag_property`, `c_verbatim_tag`,
      `c_ns_shorthand_tag`, `c_non_specific_tag`,
      `c_ns_anchor_property`, `ns_anchor_char`,
      `ns_anchor_name`
- [ ] Unit tests: indentation at various depths, comment
      preservation, directive parsing, tag/anchor extraction

### Task 4: Flow style productions (§7)

Implement the 58 flow style productions — scalars (plain,
single-quoted, double-quoted), flow sequences, and flow
mappings. Flow collections are the #1 failure area across
all parsers.

**Files:** `src/flow.rs`

- [ ] Alias nodes: `c_ns_alias_node` — wrapped in
      `BeginAlias`/`EndAlias` tokens with anchor name
- [ ] Empty nodes: `e_node`, `e_scalar`
- [ ] Flow scalars:
  - Double-quoted: `c_double_quoted(n,c)`,
    `nb_double_char`, `ns_double_char`,
    `c_double_quoted(n,c)`, multi-line with `nb_ns_double_in_line`,
    `s_double_next_line(n)`, `nb_double_multi_line(n)`
  - Single-quoted: `c_single_quoted(n,c)`,
    `nb_single_char`, `ns_single_char`,
    `c_quoted_quote`, multi-line variants
  - Plain: `ns_plain(n,c)`, `ns_plain_one_line(c)`,
    `ns_plain_multi_line(n,c)`, `s_ns_plain_next_line(n,c)`,
    `nb_ns_plain_in_line(c)` — context-sensitive (flow
    indicators forbidden in flow context)
- [ ] Flow sequences: `c_flow_sequence(n,c)`,
      `ns_s_flow_seq_entries(n,c)`,
      `ns_flow_seq_entry(n,c)`
- [ ] Flow mappings: `c_flow_mapping(n,c)`,
      `ns_s_flow_map_entries(n,c)`,
      `ns_flow_map_entry(n,c)`,
      `ns_flow_map_explicit_entry(n,c)`,
      `ns_flow_map_implicit_entry(n,c)`,
      `ns_flow_map_yaml_key_entry(n,c)`,
      `ns_flow_map_empty_key_entry(n,c)`,
      `c_ns_flow_map_separate_value(n,c)`,
      `c_ns_flow_map_json_key_entry(n,c)`,
      `c_ns_flow_map_adjacent_value(n,c)`,
      `ns_flow_pair(n,c)`,
      `ns_flow_pair_entry(n,c)`,
      `ns_flow_pair_yaml_key_entry(n,c)`,
      `c_ns_flow_pair_json_key_entry(n,c)`,
      `c_s_implicit_json_key(c)`
- [ ] Flow-in-block: `ns_flow_node(n,c)`,
      `c_ns_flow_content(n,c)`,
      `ns_flow_content(n,c)`,
      `ns_flow_yaml_content(n,c)`,
      `c_flow_json_content(n,c)`,
      `ns_flow_yaml_node(n,c)`,
      `c_flow_json_node(n,c)`,
      `ns_flow_pair_yaml_key_entry`
- [ ] Unit tests: nested flow collections, colon-adjacent
      values, multiline flow keys, empty collections,
      mixed flow/block

### Task 5: Block style productions (§8)

Implement the 40 block style productions — block scalars
with full chomping support, block sequences, and block
mappings.

**Files:** `src/block.rs`

- [ ] Block scalar headers: `c_b_block_header(n)`,
      `c_indentation_indicator(n)`,
      `c_chomping_indicator` — parse and preserve Strip,
      Clip, Keep chomping indicators
- [ ] Literal block scalars: `c_l_literal(n)`,
      `l_literal_content(n,t)`, `l_nb_literal_text(n)`,
      `b_nb_literal_next(n)`, `b_chomped_last(t)`,
      `l_chomped_empty(n,t)`, `l_strip_empty(n)`,
      `l_keep_empty(n)`, `l_trail_comments(n)` —
      auto-detect indentation, handle all three chomping
      modes
- [ ] Folded block scalars: `c_l_folded(n)`,
      `l_folded_content(n,t)`, `s_nb_folded_text(n)`,
      `s_nb_folded_lines(n)`, `s_nb_spaced_text(n)`,
      `s_nb_spaced_lines(n)`, `l_nb_same_lines(n)`,
      `l_nb_diff_lines(n)`, `b_l_folded(n,c)` — line
      folding rules
- [ ] Block sequences: `l_block_sequence(n)`,
      `c_l_block_seq_entry(n)`,
      `s_b_block_indented(n,c)`,
      `ns_l_compact_sequence(n)`
- [ ] Block mappings: `l_block_mapping(n)`,
      `ns_l_block_map_entry(n)`,
      `c_l_block_map_explicit_entry(n)`,
      `c_l_block_map_explicit_key(n)`,
      `l_block_map_explicit_value(n)`,
      `ns_l_block_map_implicit_entry(n)`,
      `ns_s_block_map_implicit_key`,
      `c_l_block_map_implicit_value(n)`,
      `ns_l_compact_mapping(n)`
- [ ] Block nodes: `s_l_block_node(n,c)`,
      `s_l_flow_in_block(n)`, `s_l_block_in_block(n,c)`,
      `s_l_block_scalar(n,c)`, `s_l_block_collection(n,c)`,
      `s_l_block_indented(n,c)`,
      `l_block_content(n,c)`
- [ ] Unit tests: all three chomping modes, auto-detected
      vs explicit indentation, nested block collections,
      compact notation, block-in-flow

### Task 6: Document stream productions (§9)

Implement the 10 document stream productions — document
boundaries, bare and explicit documents, multi-document
streams.

**Files:** `src/stream.rs`

- [ ] Document markers: `c_directives_end` (`---`),
      `c_document_end` (`...`),
      `l_document_prefix`,
      `c_forbidden` — detection of document boundary
      markers in bare content
- [ ] Document types: `l_bare_document`,
      `l_explicit_document`, `l_directive_document`
- [ ] Stream: `l_any_document`, `l_yaml_stream` — the
      top-level production that drives the entire parse
- [ ] Multi-document handling: consecutive documents with
      and without explicit markers
- [ ] Unit tests: single document, multi-document with
      `---`/`...`, bare documents, directive documents,
      empty documents, document-end markers in content

### Task 7: Event layer

Build the token-to-event conversion layer — the primary
public streaming API.

**Files:** `src/event.rs`

- [ ] Event types: `StreamStart`, `StreamEnd`,
      `DocumentStart(explicit, version, tags)`,
      `DocumentEnd(explicit)`, `MappingStart(anchor, tag)`,
      `MappingEnd`, `SequenceStart(anchor, tag)`,
      `SequenceEnd`, `Scalar(value, style, anchor, tag)`,
      `Alias(name)`, `Comment(text)` — each paired with
      `Pos`
- [ ] `ScalarStyle` enum: `Plain`, `SingleQuoted`,
      `DoubleQuoted`, `Literal(Chomp)`, `Folded(Chomp)` —
      preserves original style and chomping
- [ ] Token-to-event state machine: consume filtered
      token stream, emit events. Handle Begin/End token
      pairs, accumulate Text tokens into scalar values,
      track anchors and tags from token metadata
- [ ] Public API: `fn parse_events(input: &str) -> impl
      Iterator<Item = Result<(Event, Pos), Error>>` —
      streaming iterator, no need to parse entire input
      before yielding first event
- [ ] Error recovery: on parse error, emit error event
      with position and message, attempt to continue
      parsing from next document boundary
- [ ] Unit tests: event sequence for simple mapping, nested
      structures, multi-document, scalars with all styles,
      anchors/aliases, comments, error recovery

### Task 8: AST loader

Build the event-to-node-graph layer — constructs the YAML
AST from the event stream.

**Files:** `src/loader.rs`, `src/node.rs`

- [ ] `Node` type parameterized by location: `Node<Loc>` —
      variants: `Scalar(value, style, tag)`,
      `Mapping(Vec<(Node<Loc>, Node<Loc>)>)`,
      `Sequence(Vec<Node<Loc>>)`, `Alias(anchor_name)` —
      each carrying `Loc` (typically `Span`)
- [ ] Loader consumes event iterator, builds node graph
- [ ] Anchor registration: store anchor → node mapping
      during construction
- [ ] Alias resolution: configurable — either resolve
      aliases inline (saphyr compat) or preserve them as
      `Alias` nodes (lossless mode for LSP)
- [ ] Cycle detection: track active anchors to detect and
      report circular references
- [ ] Multi-document: return `Vec<Node<Loc>>` for document
      sequence
- [ ] Comment attachment: associate comments with adjacent
      nodes (preceding or trailing)
- [ ] Public API: `fn load(input: &str) -> Result<Vec<
      Document<Span>>, Error>` where `Document` wraps a
      root node + directives metadata
- [ ] Unit tests: simple and nested structures, anchors
      and aliases (including forward references), cycles,
      multi-document, comment attachment

### Task 9: Schema resolution

Implement YAML schema resolution — tag resolution and
scalar type inference for failsafe, JSON, and core schemas.

**Files:** `src/schema.rs`

- [ ] Schema trait: pluggable schema resolution strategy
- [ ] Failsafe schema: all scalars are strings, mappings
      are unordered, sequences are ordered — no type
      inference
- [ ] JSON schema: null (`null`), bool (`true`/`false`),
      int (decimal), float (decimal with `.` or `e`/`E`),
      string (everything else)
- [ ] Core schema (default): extends JSON with additional
      patterns — null (`~`, empty), bool (`True`/`False`,
      `TRUE`/`FALSE`), int (octal `0o`, hex `0x`), float
      (`.inf`, `.nan`), plus unquoted string fallback
- [ ] Tag resolution: resolve shorthand tags (`!!str`,
      `!!int`, etc.) against tag prefixes from directives,
      handle verbatim tags (`!<uri>`), non-specific tags
      (`!`, `?`)
- [ ] `Scalar` value type: `Null`, `Bool(bool)`,
      `Int(i64)`, `Float(f64)`, `String(String)` — resolved
      from raw scalar text + tag + schema
- [ ] Public API: `fn resolve(node: &Node<Loc>, schema:
      &Schema) -> ResolvedNode<Loc>`
- [ ] Unit tests: each schema's resolution rules, tag
      precedence, edge cases (`.inf`, `0o777`, `~`,
      empty scalar, quoted scalars bypass inference)

### Task 10: Emitter

Implement YAML serialization — node graph to YAML text
with style and comment preservation.

**Files:** `src/emitter.rs`

- [ ] Emitter configuration: indent width, line width,
      default scalar style, default collection style
      (block vs flow)
- [ ] Scalar emission: respect `ScalarStyle` from node —
      plain, single-quoted, double-quoted, literal block,
      folded block (with correct chomping)
- [ ] Mapping emission: block style (key: value with
      indentation) and flow style ({key: value})
- [ ] Sequence emission: block style (- item) and flow
      style ([item1, item2])
- [ ] Comment emission: preserve comments in their
      original positions relative to nodes
- [ ] Anchor/alias emission: emit `&anchor` on first
      occurrence, `*anchor` on aliases
- [ ] Multi-document emission: `---` separators, `...`
      terminators, directive preambles
- [ ] Tag emission: shorthand and verbatim tags
- [ ] Public API: `fn emit(documents: &[Document<Loc>],
      config: &EmitConfig) -> String`
- [ ] Unit tests: round-trip (parse → emit → re-parse →
      compare) for all node types, style preservation,
      comment preservation, multi-document

### Task 11: YAML test suite conformance

Integrate the YAML test suite and drive conformance to
100%.

**Files:** `tests/conformance.rs`, `tests/yaml-test-suite/`

- [ ] Vendor the YAML test suite (same commit as
      rlsp-yaml's existing vendored copy, or update both
      to latest)
- [ ] Event comparison tests: parse each test case, compare
      emitted event stream against expected events from
      test suite
- [ ] Invalid input tests: verify parser rejects all
      invalid test cases (no false accepts)
- [ ] Track conformance metrics: pass/fail/skip counts,
      categorized by spec section and failure type
- [ ] Fix failures iteratively — the productions from
      tasks 2–6 will likely need refinement based on test
      results. Common failure areas to focus on:
  - Flow collections (most common cross-parser failures)
  - Document/directive handling
  - Mapping edge cases (multiline keys, colon-adjacent)
  - Indentation edge cases
  - Tab handling
- [ ] Target: 308/308 valid, 94/94 invalid (100%/100%)
- [ ] Conformance CI: test suite runs in CI, regressions
      block merge

### Task 12: Benchmarks and libfyaml comparison

Add comprehensive benchmarks with libfyaml as the initial
performance baseline.

**Files:** `benches/throughput.rs`, `benches/latency.rs`,
`benches/memory.rs`, `benches/fixtures.rs`,
devcontainer config updates

- [ ] Install libfyaml in devcontainer (add to Dockerfile:
      build from source or install package)
- [ ] libfyaml FFI bindings: minimal `extern "C"` bindings
      for `fy_parse_load_string` / event iteration — just
      enough for benchmark comparison, not a full binding
- [ ] Throughput benchmark: parse MB/s across document
      sizes (tiny ~100B, medium ~10KB, large ~100KB, huge
      ~1MB) and styles (block-heavy, flow-heavy, scalar-
      heavy, mixed). Compare rlsp-yaml-parser vs libfyaml.
- [ ] Latency benchmark: time-to-first-event for
      streaming parse. Measures responsiveness for LSP use
      case where partial results matter.
- [ ] Memory benchmark: peak allocation during parse of
      large documents. Use a custom allocator or
      `jemalloc-ctl` stats.
- [ ] Fixture generation: reusable module generating
      synthetic YAML at various sizes and complexity levels
- [ ] Criterion integration: HTML reports, statistical
      significance testing, regression detection
- [ ] Document baseline results in crate README

## Decisions

- **From scratch, not a saphyr fork.** Saphyr's scanner is
  a libyaml-style state machine that doesn't follow the
  spec productions. The conformance gaps (89.6% valid,
  70.2% invalid) and structural limitations (no comments,
  no container spans, eager alias resolution) are
  architectural — fixing them means rewriting the core.
  Starting fresh lets us build spec-faithful productions
  from the ground up.

- **Spec-transliteration approach.** Each of the 211 YAML
  1.2 grammar productions maps to a parser combinator
  function, cross-referenced by production number. This is
  the approach HsYAML uses (97.1%/100% conformance). It
  makes the parser auditable against the spec — when a test
  fails, you can trace it to the exact production.

- **HsYAML as reference only.** HsYAML is GPL-2.0 — no
  translation. We implement from the YAML 1.2 spec. HsYAML
  is consulted only when the spec is ambiguous. The spec's
  formal grammar is the shared upstream source of structure;
  similarity to HsYAML in production layout is expected and
  legally unproblematic since both derive from the same
  spec.

- **Comments as first-class tokens.** Unlike saphyr (which
  discards comments), comments get `BeginComment`/
  `EndComment` token pairs and propagate to Comment events.
  This eliminates the rlsp-yaml formatter's expensive
  comment extract/reattach workaround.

- **Aliases preserved in AST.** The loader offers both
  resolved mode (saphyr compat) and lossless mode (aliases
  as nodes). Lossless mode enables the LSP to navigate
  anchor/alias relationships.

- **libfyaml benchmark is temporary.** Installed in
  devcontainer with minimal FFI bindings. Once we establish
  our own throughput baselines and are satisfied with
  performance, the libfyaml dependency is removed. No -sys
  crate — just raw `extern "C"` in the bench file.

- **Crate name: `rlsp-yaml-parser`.** Workspace crate at
  project root, publishable to crates.io. Advertises the
  rlsp project; users can depend on the parser without the
  full language server.
