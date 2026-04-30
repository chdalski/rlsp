---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: A
section: §6
date: 2026-04-30
---

### [63] s-indent(n)

BNF:
```
s-indent(0) ::=
  <empty>

# When n≥0
s-indent(n+1) ::=
  s-space s-indent(n)
```

Spec prose: §6.1: "indentation is defined as a zero or more space characters at the start of a line. To maintain portability, tab characters must not be used in indentation."

Verdict: Stricter-than-spec

Evidence: `lines.rs:140-143`, `lines.rs:73-76`, `event_iter/step.rs:38-62`.

Reasoning: The parser materializes `s-indent(n)` as `Line::indent`, computed as `content.chars().take_while(|&ch| ch == ' ').count()` (`lines.rs:142`). Tabs are deliberately not counted (`lines.rs:73-76` doc comment: "Leading tabs do not contribute to indent — they are a YAML syntax error in indentation context"). At the document body level, `step.rs:48-60` rejects any line whose first character is a tab with the error "tabs are not allowed as indentation (YAML 1.2 §6.1)." The spec says tabs "must not be used in indentation" but the productions themselves admit zero or more `s-space` characters and treat tab handling as an interpretive constraint — most YAML parsers accept tabs in non-indenting whitespace contexts. The hard rejection of any leading tab on a content line, including on lines like `\t-` that would not actually claim indentation under a precise reading of `s-indent`, is stricter than the BNF mechanically requires. The strictness aligns with the §6.1 prose, not the production itself.

### [64] s-indent-less-than(n)

BNF:
```
s-indent-less-than(1) ::=
  <empty>

# When n≥1
s-indent-less-than(n+1) ::=
  s-space s-indent-less-than(n)
  | <empty>
```

Spec prose: §6.1: "A block style construct is terminated when encountering a line which is less indented than the construct. The productions use the notation `s-indent-less-than(n)` … to express this."

Verdict: Strict-conformant

Evidence: `lexer/plain.rs:185-189`, `event_iter/block/sequence.rs` (loop dedent checks), `lines.rs:319-375`.

Reasoning: The parser implements `s-indent-less-than(n)` as the indent-comparison `next.indent <= parent_indent` for break (`plain.rs:187-189`) and `line.indent <= base_indent` in `peek_until_dedent` (`lines.rs:367-369`). The spec defines this as "zero to n-1 spaces"; mechanically the parser detects construct termination when a non-blank line's indent satisfies the corresponding inequality. Blank lines are treated transparently in `peek_until_dedent` (`lines.rs:359-362`), matching the spec's note that empty lines are subject to `s-indent-less-than` *or* `s-line-prefix(n,c)` per `l-empty` ([67]).

### [65] s-indent-less-or-equal(n)

BNF:
```
s-indent-less-or-equal(0) ::=
  <empty>

# When n≥0
s-indent-less-or-equal(n+1) ::=
  s-space s-indent-less-or-equal(n)
  | <empty>
```

Spec prose: §6.1: same as [64] but the inequality is `≤ n` rather than `< n`.

Verdict: Strict-conformant

Evidence: `lexer/plain.rs:185-189`, `lines.rs:367-369`.

Reasoning: This production is used by the spec for the same termination concept with a `≤` boundary (e.g. mapping value chomping). The parser uses `<=` consistently in dedent-comparison checks (`plain.rs:187`: `if next.indent <= parent_indent`; `lines.rs:367`: `line.indent <= base_indent`). The choice of which inequality to use at each call site is governed by the parser's higher-level state machine — for example, plain-scalar continuations stop at `<= parent_indent`, matching the spec's expectations for `s-indent-less-or-equal(n)`.

### [66] s-separate-in-line

BNF:
```
s-separate-in-line ::=
    s-white+
  | <start-of-line>
```

Spec prose: §6.2: "Outside indentation and scalar content, YAML uses white space characters for separation between tokens within a line. Note that such white space may safely include tab characters."

Verdict: Strict-conformant

Evidence: `lexer/plain.rs:159` (`trim_start_matches([' ', '\t'])`), `lexer.rs:181-183` (comment line detection with leading whitespace), `event_iter/directives.rs:90-93,123,165-170` (directive name/parameter separation), `event_iter/step.rs:483-484` (tag-to-content separation).

Reasoning: `s-white` is `space | tab` per §5.5. The parser uses `trim_start_matches([' ', '\t'])` consistently when consuming inter-token whitespace (e.g. directive parameter splitting at `directives.rs:90,123,165`; tag/anchor inline trimming at `step.rs:483-484`; comment indent at `lexer.rs:181`). The `<start-of-line>` alternative is implicit: `LineBuffer` produces line-aligned slices, and at column 0 a token can begin without preceding whitespace. The `s-white+` form is enforced where required (e.g. comment must be preceded by whitespace — `lexer/plain.rs:516-531`).

### [67] s-line-prefix(n,c)

BNF:
```
s-line-prefix(n,BLOCK-OUT) ::= s-block-line-prefix(n)
s-line-prefix(n,BLOCK-IN)  ::= s-block-line-prefix(n)
s-line-prefix(n,FLOW-OUT)  ::= s-flow-line-prefix(n)
s-line-prefix(n,FLOW-IN)   ::= s-flow-line-prefix(n)
```

Spec prose: §6.3: "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation. For flow scalar styles it additionally includes all leading white space, which may contain tab characters."

Verdict: Strict-conformant

Evidence: `lexer/plain.rs:159,191`, `lexer/quoted.rs:107`, `lexer/block.rs:396-407`.

Reasoning: This production is a context-dependent dispatch to `s-block-line-prefix` ([68]) or `s-flow-line-prefix` ([69]). The parser routes each scalar style to the appropriate prefix-stripping logic: block scalars call `line_content.get(content_indent..)` (`block.rs:396-397`); plain block scalars trim the line then check indent threshold (`plain.rs:159,191`); quoted scalars strip leading whitespace including tabs (`quoted.rs:107`). The dispatch is correct because the parser tracks `c` (BLOCK-IN/OUT vs FLOW-IN/OUT) implicitly through the choice of consumer function.

### [68] s-block-line-prefix(n)

BNF:
```
s-block-line-prefix(n) ::=
  s-indent(n)
```

Spec prose: §6.3 (continued): in block context the line prefix is exactly the indentation.

Verdict: Strict-conformant

Evidence: `lexer/block.rs:396-407`.

Reasoning: Block scalar content strips exactly `content_indent` columns of spaces from the line: `line_content.get(content_indent..).unwrap_or("")` (`block.rs:396-397`). The `is_content_line` predicate (`block.rs:406-407`) uses `next.indent >= content_indent` to ensure the indent matches `s-indent(n)`. No tab content is consumed as part of the prefix (`block.rs:368-381` rejects tabs in indentation), matching `s-indent(n) ::= s-space s-indent(n-1)`.

### [69] s-flow-line-prefix(n)

BNF:
```
s-flow-line-prefix(n) ::=
  s-indent(n)
  s-separate-in-line?
```

Spec prose: §6.3: "For flow scalar styles it additionally includes all leading white space, which may contain tab characters."

Verdict: Strict-conformant

Evidence: `lexer/quoted.rs:107`, `lexer/plain.rs:159` (continuation lines).

Reasoning: For quoted scalars the continuation-line prefix is computed by `let trimmed = line_content.trim_start_matches([' ', '\t'])` (`quoted.rs:107`), which strips both spaces (`s-indent`) and tabs (`s-separate-in-line`) in one pass. Plain scalars do the same (`plain.rs:159`). This matches the BNF: `s-indent(n)` followed by an optional run of `s-white` characters. The block-context indent constraint is checked separately for quoted continuations (`quoted.rs:282-291`).

### [70] l-empty(n,c)

BNF:
```
l-empty(n,c) ::=
  (
      s-line-prefix(n,c)
    | s-indent-less-than(n)
  )
  b-as-line-feed
```

Spec prose: §6.4: "An empty line consists of the non-content prefix followed by a line break."

Verdict: Strict-conformant

Evidence: `lexer.rs:521-523` (`is_blank_not_comment`), `lexer/plain.rs:161-168`, `lexer/quoted.rs:109-113`, `lexer/block.rs:447-470`.

Reasoning: Empty-line handling is consistent across consumers: a line is "empty" when `line.content.trim_start_matches([' ', '\t']).is_empty()` (`lexer.rs:522`). This accepts either the `s-line-prefix` form (n spaces of indent) or the `s-indent-less-than` form (fewer than n spaces) — both produce a `trim` result of empty. The terminator (`b-as-line-feed`) is captured by `LineBuffer` as `Line::break_type` and contributes a `\n` to the folded output (`block.rs:455-470` increments trailing-newline counter; `plain.rs:162` counts pending blanks). Block-scalar indent constraint on blank lines (`block.rs:455-470`) enforces that `l-empty` accepts at most `n` spaces.

### [71] b-l-trimmed(n,c)

BNF:
```
b-l-trimmed(n,c) ::=
  b-non-content
  l-empty(n,c)+
```

Spec prose: §6.5: "If a line break is followed by an empty line, it is trimmed; the first line break is discarded and the rest are retained as content."

Verdict: Strict-conformant

Evidence: `lexer/block.rs:417-422`, `lexer/quoted.rs:308-318`, `lexer/plain.rs:210-215`.

Reasoning: All three folding contexts implement the trim rule: when one or more blank lines follow a line break, the first break is discarded and the subsequent breaks are emitted as `\n`. In `block.rs:417-422`: `out.extend(std::iter::repeat_n('\n', trailing_newlines + extra))` where `trailing_newlines` is the count of empty lines (the first break is consumed by entering blank state, the rest are emitted). In `plain.rs:210-212`: `buf.extend(std::iter::repeat_n('\n', pending_blanks))` (one fewer than the total newlines because the first is folded). In `quoted.rs:308-318`: equivalent logic with `pending_blanks`.

### [72] b-as-space

BNF:
```
b-as-space ::=
  b-break
```

Spec prose: §6.5: "Otherwise (the following line is not empty), the line break is converted to a single space (x20)."

Verdict: Strict-conformant

Evidence: `lexer/plain.rs:213-215`, `lexer/quoted.rs:117-119,313`, `lexer/block.rs:427-429`.

Reasoning: Each folding context maps a single break between non-empty lines to a single space. Plain scalar: `buf.push(' ')` (`plain.rs:214`). Quoted scalar: `if !owned.ends_with('\n') { owned.push(' '); }` (`quoted.rs:117-119`); also `quoted.rs:313` has the equivalent in `collect_double_quoted_continuations`. Block folded scalar: `out.push(' ')` (`block.rs:428`).

### [73] b-l-folded(n,c)

BNF:
```
b-l-folded(n,c) ::=
  b-l-trimmed(n,c) | b-as-space
```

Spec prose: §6.5: "A folded non-empty line may end with either of the above line breaks."

Verdict: Strict-conformant

Evidence: `lexer/block.rs:416-429`, `lexer/plain.rs:210-215`.

Reasoning: This production is the union of [71] and [72]. The parser dispatches at the call site by inspecting the trailing-blanks counter: `if trailing_newlines > 0 { ... b-l-trimmed branch ... } else if prev_more_indented || is_more_indented { out.push('\n') } else { out.push(' ') ... b-as-space branch ... }` (`block.rs:417-429`). Plain scalar: `if pending_blanks > 0 { ...newlines... } else { buf.push(' ') }` (`plain.rs:210-214`). Both [71] and [72] are correctly composed.

### [74] s-flow-folded(n)

BNF:
```
s-flow-folded(n) ::=
  s-separate-in-line?
  b-l-folded(n,FLOW-IN)
  s-flow-line-prefix(n)
```

Spec prose: §6.5 Flow Folding: "spaces preceding or following the text in a line are a presentation detail and must not be used to convey content information. Once all such spaces have been discarded, all line breaks are folded without exception."

Verdict: Strict-conformant

Evidence: `lexer/quoted.rs:152` (`trim_end_matches([' ', '\t'])`), `lexer/quoted.rs:107` (leading whitespace strip), `lexer/quoted.rs:117-119` (fold to space), `lexer/plain.rs:159,180-182,210-215`.

Reasoning: For flow scalars (single-quoted, double-quoted), continuation lines have leading whitespace stripped (`quoted.rs:107`), the trailing whitespace of the previous line is trimmed (`quoted.rs:152` trims `[' ', '\t']` from the end), and the line break is folded per `b-l-folded`. The plain-scalar continuation logic at `plain.rs:180-182` documents the n=0 special case for `s-flow-folded(0)` and applies fold-to-space at `plain.rs:213-214`.

### [75] c-nb-comment-text

BNF:
```
c-nb-comment-text ::=
  c-comment    # '#'
  nb-char*
```

Spec prose: §6.6: "An explicit comment is marked by a `#` indicator."

Verdict: Strict-conformant

Evidence: `lexer/comment.rs:30-72`, `lexer/plain.rs:516-531`.

Reasoning: `try_consume_comment` (`comment.rs:22-73`) locates the `#` (`comment.rs:37`) and returns the slice `&line.content[text_start..]` (`comment.rs:51`) — i.e., everything after the `#` up to the line terminator (which `LineBuffer` excludes from `Line::content`). This matches `c-comment` followed by `nb-char*` (non-break characters). `extract_trailing_comment` (`plain.rs:516-531`) does the same for in-line comments. NUL bytes are caught downstream (`plain.rs:81-91`) because `nb-char` is ns-printable minus break which excludes NUL.

### [76] b-comment

BNF:
```
b-comment ::=
    b-non-content
  | <end-of-input>
```

Spec prose: §6.6: "YAML processors must allow for the omission of the final comment line break of the input stream."

Verdict: Strict-conformant

Evidence: `lines.rs:91-102` (`detect_break`), `lines.rs:36-40` (`BreakType::Eof`), `lexer/comment.rs:62-72`.

Reasoning: A comment is terminated by either a real line break or end-of-input. `LineBuffer::scan_line` (`lines.rs:110-157`) detects the break with `detect_break` (`lines.rs:91-102`) producing one of `Lf`/`Cr`/`CrLf`/`Eof`. `try_consume_comment` consumes the entire line including its terminator regardless of which `BreakType` it carries (`comment.rs:67-70`). EOF-terminated comments are accepted because `Line::break_type == BreakType::Eof` is a valid line.

### [77] s-b-comment

BNF:
```
s-b-comment ::=
  (
    s-separate-in-line
    c-nb-comment-text?
  )?
  b-comment
```

Spec prose: §6.6: "Comments must be separated from other tokens by white space characters."

Verdict: Strict-conformant

Evidence: `lexer/plain.rs:516-531`, `lexer/plain.rs:60-95`.

Reasoning: For comments that follow content on the same line, `extract_trailing_comment` enforces the whitespace requirement: `let preceded_by_ws = i == 0 || matches!(bytes.get(i - 1), Some(b' ' | b'\t'))` (`plain.rs:523`). When `#` appears without preceding whitespace (e.g. `foo#bar`), the function continues searching, treating the `#` as part of the scalar — matching the spec rule that comments must be separated by white space. The optional `s-separate-in-line` is satisfied by the at-least-one whitespace check.

### [78] l-comment

BNF:
```
l-comment ::=
  s-separate-in-line
  c-nb-comment-text?
  b-comment
```

Spec prose: §6.6: "Outside scalar content, comments may appear on a line of their own, independent of the indentation level. Note that outside scalar content, a line containing only white space characters is taken to be a comment line."

Verdict: Strict-conformant

Evidence: `lexer.rs:179-184`, `lexer/comment.rs:30-44`, `lexer.rs:521-523`.

Reasoning: A standalone comment line is detected by `is_comment_line`: `line.content.trim_start_matches([' ', '\t']).starts_with('#')` (`lexer.rs:181-183`). The leading whitespace satisfies `s-separate-in-line` (admitting tabs and spaces; the `<start-of-line>` alternative also satisfies it for an unindented comment). Whitespace-only lines are accepted as blank in `is_blank_not_comment` (`lexer.rs:521-523`), which by spec are "taken to be a comment line." The `try_consume_comment` path emits these as `Event::Comment` (`directives.rs:40-43`).

### [79] s-l-comments

BNF:
```
s-l-comments ::=
  (
      s-b-comment
    | <start-of-line>
  )
  l-comment*
```

Spec prose: §6.6: "In most cases, when a line may end with a comment, YAML allows it to be followed by additional comment lines. The only exception is a comment ending a block scalar header."

Verdict: Strict-conformant

Evidence: `event_iter/directives.rs:33-64,237-256`, `lexer.rs:104-118`.

Reasoning: After consuming a same-line trailing comment, the parser loops: `consume_preamble_between_docs` (`directives.rs:33-64`) and `skip_and_collect_comments_in_doc` (`directives.rs:237-256`) repeatedly skip blank lines and collect comment lines until non-blank, non-comment content is reached. This implements `l-comment*`. The `<start-of-line>` alternative is satisfied at the document boundary where no preceding content exists. Block scalar headers do not enter the multi-line comment loop (their headers are parsed by `parse_block_header` with single-line trailing comment handling), satisfying the "only exception" carve-out.

### [80] s-separate(n,c)

BNF:
```
s-separate(n,BLOCK-OUT) ::= s-separate-lines(n)
s-separate(n,BLOCK-IN)  ::= s-separate-lines(n)
s-separate(n,FLOW-OUT)  ::= s-separate-lines(n)
s-separate(n,FLOW-IN)   ::= s-separate-lines(n)
s-separate(n,BLOCK-KEY) ::= s-separate-in-line
s-separate(n,FLOW-KEY)  ::= s-separate-in-line
```

Spec prose: §6.7: "Implicit keys are restricted to a single line. In all other cases, YAML allows tokens to be separated by multi-line (possibly empty) comments."

Verdict: Strict-conformant

Evidence: `event_iter/step.rs:474-516`, `event_iter/flow.rs:1262`, `event_iter/directives.rs:33-64`.

Reasoning: This production dispatches between `s-separate-lines(n)` (allowing multi-line separation including comments) and `s-separate-in-line` (single-line only) based on context `c`. The parser does not name the production explicitly but enforces the right behavior at each call site: between tag and node content (`step.rs:483,502-516`), the parser allows a single-line separator (since this is property-to-content separation within a block-key or flow-key context); within document preambles (`directives.rs:33-64`), it allows multi-line `s-l-comments`. Implicit-key handling is restricted to single lines because `find_value_indicator_offset` operates on a single line (this is enforced in `event_iter/block/mapping.rs`).

### [81] s-separate-lines(n)

BNF:
```
s-separate-lines(n) ::=
    (
      s-l-comments
      s-flow-line-prefix(n)
    )
  | s-separate-in-line
```

Spec prose: §6.7: "structures following multi-line comment separation must be properly indented, even though there is no such restriction on the separation comment lines themselves."

Verdict: Strict-conformant

Evidence: `event_iter/directives.rs:237-256`, `lexer.rs:104-118`.

Reasoning: The parser's multi-line separation strategy (`skip_and_collect_comments_in_doc`) consumes blank lines and comment lines without enforcing indent constraints on the comments themselves (`directives.rs:241-256` calls `skip_empty_lines` which is indent-agnostic). The next non-blank line's indent is then validated against the structural context. This matches the BNF: `s-l-comments` (no indent constraint on the comments) followed by `s-flow-line-prefix(n)` (indent enforced on the next structural token).

### [82] l-directive

BNF:
```
l-directive ::=
  c-directive            # '%'
  (
      ns-yaml-directive
    | ns-tag-directive
    | ns-reserved-directive
  )
  s-l-comments
```

Spec prose: §6.8: "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name and a list of parameters."

Verdict: Strict-conformant

Evidence: `lexer.rs:150-174`, `event_iter/directives.rs:51-103`.

Reasoning: `try_consume_directive_line` accepts only lines that begin with `%` at column 0 (`lexer.rs:163-165` checks `line.content.starts_with('%')`; lines from `LineBuffer` always start at the original column 0). `parse_directive` (`directives.rs:70-104`) then dispatches to `parse_yaml_directive`, `parse_tag_directive`, or treats unknown names as reserved (`directives.rs:98-103`). After each directive, `consume_preamble_between_docs` resumes its blank/comment/directive loop (`directives.rs:34-64`), satisfying the trailing `s-l-comments`. The "non-indented" requirement is implicit because indentation would push the `%` past column 0 and `try_consume_directive_line` would not match.

### [83] ns-reserved-directive

BNF:
```
ns-reserved-directive ::=
  ns-directive-name
  (
    s-separate-in-line
    ns-directive-parameter
  )*
```

Spec prose: §6.8: "A YAML processor should ignore unknown directives with an appropriate warning."

Verdict: Lenient

Evidence: `event_iter/directives.rs:98-103`.

Reasoning: The parser ignores unknown directives without parsing their parameters or emitting any diagnostic: `_ => { self.directive_scope.directive_count += 1; Ok(()) }` (`directives.rs:98-103`). The spec says implementations "should ignore unknown directives with an appropriate warning." The parser ignores silently (no warning channel), and crucially does not validate the body of the directive against `ns-directive-name` followed by `ns-directive-parameter` separated by `s-separate-in-line`. Inputs like `%FOO bad\x00content` (with a control character in what should be the parameter) would pass without the parameter-shape validation the BNF describes. The parser is lenient with respect to enforcing the production's structure.

### [84] ns-directive-name

BNF:
```
ns-directive-name ::=
  ns-char+
```

Spec prose: §6.8: implicit; the directive name is `ns-char+`.

Verdict: Lenient

Evidence: `event_iter/directives.rs:88-93`.

Reasoning: The parser extracts the directive name as `&after_percent[..name_end]` where `name_end = after_percent.find([' ', '\t']).unwrap_or(after_percent.len())` (`directives.rs:88-92`). This accepts *any* non-space, non-tab characters as the name, including line-break characters (excluded by virtue of `Line::content` excluding terminators), but also flow indicators, NUL bytes, or any other character. `ns-char` per §5.5 excludes whitespace and BOM only. The parser accepts characters that are not `ns-char` (e.g. could include U+FFFE if it were on a line). Validation is then deferred to the name-match (`directives.rs:95-103`) — only `"YAML"` and `"TAG"` are recognized; anything else falls through to silent acceptance, so malformed names are not rejected. This is Lenient compared to `ns-char+`.

### [85] ns-directive-parameter

BNF:
```
ns-directive-parameter ::=
  ns-char+
```

Spec prose: §6.8: implicit; parameters are `ns-char+` separated by `s-separate-in-line`.

Verdict: Lenient

Evidence: `event_iter/directives.rs:90-93,98-103`.

Reasoning: For reserved directives the parser does not parse parameters at all (`directives.rs:98-103` ignores body entirely), so any character sequence after the directive name including non-`ns-char` characters is silently accepted. For `%YAML` and `%TAG`, parameters are split by whitespace (`directives.rs:90-93,123,165`), but the per-parameter content is then validated by domain-specific checks (e.g. version digits at `directives.rs:136-140`, handle/prefix at `directives.rs:182-217`). The general-purpose `ns-char+` parameter shape is not enforced for the reserved-directive path, which is Lenient.

### [86] ns-yaml-directive

BNF:
```
ns-yaml-directive ::=
  "YAML"
  s-separate-in-line
  ns-yaml-version
```

Spec prose: §6.8.1: "A version 1.2 YAML processor must accept documents with an explicit `%YAML 1.2` directive… Documents with a YAML directive specifying a higher major version (e.g. `%YAML 2.0`) should be rejected with an appropriate error message."

Verdict: Strict-conformant

Evidence: `event_iter/directives.rs:107-156`.

Reasoning: `parse_yaml_directive` checks for duplicate `%YAML` directives (`directives.rs:108-113`) per the §6.8.1 rule "It is an error to specify more than one `YAML` directive for the same document." It parses major.minor as decimal digits (`directives.rs:136-143`), and rejects major != 1 with "unsupported YAML version" (`directives.rs:146-151`). The "YAML" literal match is exact (`directives.rs:96`). Separation between the literal and version is enforced by the directive-line tokenization (`directives.rs:88-93`).

### [87] ns-yaml-version

BNF:
```
ns-yaml-version ::=
  ns-dec-digit+
  '.'
  ns-dec-digit+
```

Spec prose: §6.8.1: implicit form of version literal.

Verdict: Stricter-than-spec

Evidence: `event_iter/directives.rs:116-143`.

Reasoning: The parser parses major and minor as `u8` (`directives.rs:136,140`). The spec production `ns-dec-digit+` admits arbitrary-length decimal digit sequences (e.g. `%YAML 100.999`). Parsing as `u8` rejects values > 255, which is stricter than the BNF. In practice this is also stricter than necessary because the spec only requires major-version-mismatch handling for unsupported versions; a value like `%YAML 1.300` would be rejected by `u8::parse` for the minor version. The spec says "Documents with a YAML directive specifying a higher minor version (e.g. `%YAML 1.3`) should be processed with an appropriate warning" — the parser silently accepts 1.x for any u8-parseable x. Stricter overall on the digit count.

### [88] ns-tag-directive

BNF:
```
ns-tag-directive ::=
  "TAG"
  s-separate-in-line
  c-tag-handle
  s-separate-in-line
  ns-tag-prefix
```

Spec prose: §6.8.2: "It is an error to specify more than one `TAG` directive for the same handle in the same document, even if both occurrences give the same prefix."

Verdict: Strict-conformant

Evidence: `event_iter/directives.rs:159-230`.

Reasoning: `parse_tag_directive` splits the parameters into handle and prefix on whitespace (`directives.rs:165-170`), validates the handle shape via `is_valid_tag_handle` (`directives.rs:182-187`), rejects empty prefix (`directives.rs:172-177`), enforces length limits (`directives.rs:189-205`), rejects control characters in the prefix (`directives.rs:208-215`), and rejects duplicate handles (`directives.rs:218-223`). The spec "It is an error to specify more than one TAG directive for the same handle" is enforced. The "TAG" literal match is exact (`directives.rs:97`).

### [89] c-tag-handle

BNF:
```
c-tag-handle ::=
    c-named-tag-handle
  | c-secondary-tag-handle
  | c-primary-tag-handle
```

Spec prose: §6.8.2.1: "There are three tag handle variants."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:281-295`.

Reasoning: `is_valid_tag_handle` dispatches between the three variants: `"!"` and `"!!"` are accepted directly (`properties.rs:283`), and the named-handle case is matched by stripping leading and trailing `!` and verifying the inner is non-empty word characters (`properties.rs:286-292`). The dispatch order doesn't matter because the patterns are mutually exclusive.

### [90] c-primary-tag-handle

BNF:
```
c-primary-tag-handle ::= '!'
```

Spec prose: §6.8.2.1: "The primary tag handle is a single `!` character."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:283`.

Reasoning: `match handle { "!" | "!!" => true, ... }` (`properties.rs:283`) accepts exactly the literal `!` for the primary handle (and `!!` for the secondary in the same arm). The match is on the entire string, so a value like `!x` does not satisfy this arm and falls through to the named-handle parser.

### [91] c-secondary-tag-handle

BNF:
```
c-secondary-tag-handle ::= "!!"
```

Spec prose: §6.8.2.1: "The secondary tag handle is written as `!!`."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:283`.

Reasoning: Same arm as primary — `match handle { "!" | "!!" => true, ... }`. Exact literal match against `!!`.

### [92] c-named-tag-handle

BNF:
```
c-named-tag-handle ::=
  c-tag            # '!'
  ns-word-char+
  c-tag            # '!'
```

Spec prose: §6.8.2.1: "A named tag handle surrounds a non-empty name with `!` characters."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:286-292`.

Reasoning: The parser strips leading `!` and trailing `!` (`properties.rs:287`), checks the inner is non-empty (`properties.rs:288`), and verifies every inner character satisfies `c.is_ascii_alphanumeric() || c == '-'` (`properties.rs:289`). This matches `ns-word-char` per §5.6 which is `ns-dec-digit | ns-ascii-letter | "-"` (decimal digit + ASCII letter + hyphen). No underscore is allowed (verified by tests `properties.rs:486-504`).

### [93] ns-tag-prefix

BNF:
```
ns-tag-prefix ::=
  c-ns-local-tag-prefix | ns-global-tag-prefix
```

Spec prose: §6.8.2.2: "There are two tag prefix variants."

Verdict: Lenient

Evidence: `event_iter/directives.rs:170-216`.

Reasoning: The parser does not distinguish between `c-ns-local-tag-prefix` (prefix begins with `!`, body is `ns-uri-char*`) and `ns-global-tag-prefix` (begins with `ns-tag-char`, body is `ns-uri-char*`). It accepts any non-empty prefix (`directives.rs:172-177`) and only rejects control characters and lengths (`directives.rs:200-215`). The BNF requires that, after the leading character, the body consists of `ns-uri-char` (specifically excluding spaces, flow indicators, and other non-URI characters), but the parser does not validate URI-character correctness in tag-directive prefixes — only lengths and control-character exclusion. A prefix containing `<`, `^`, or other URI-illegal characters is silently accepted. This is Lenient.

### [94] c-ns-local-tag-prefix

BNF:
```
c-ns-local-tag-prefix ::=
  c-tag           # '!'
  ns-uri-char*
```

Spec prose: §6.8.2.2: "If the prefix begins with a `!` character, shorthands using the handle are expanded to a local tag."

Verdict: Lenient

Evidence: `event_iter/directives.rs:170-216`.

Reasoning: The parser does not validate that the body of a `!`-prefixed tag-directive prefix consists of `ns-uri-char` characters. As above (`directives.rs:200-215` only rejects control chars and length overruns), local prefixes like `!foo bar` would be split by the whitespace handling (`directives.rs:165-170`), but `!foo<bar` or `!foo[bar]` would be accepted. The spec restricts the body to URI characters; the parser does not enforce this. Lenient with respect to the body grammar.

### [95] ns-global-tag-prefix

BNF:
```
ns-global-tag-prefix ::=
  ns-tag-char
  ns-uri-char*
```

Spec prose: §6.8.2.2: "If the prefix begins with a character other than `!`, it must be a valid URI prefix, and should contain at least the scheme."

Verdict: Lenient

Evidence: `event_iter/directives.rs:170-216`.

Reasoning: The parser accepts any non-`!`-prefixed prefix as a global-tag prefix without validating that the leading character is `ns-tag-char` (which excludes flow indicators) or that the remainder is `ns-uri-char*`. A prefix like `foo bar` would be split (whitespace truncates), but `,foo`, `[foo`, or `{foo` are not rejected by the prefix shape — they would be accepted as global prefixes. Lenient on the leading-character constraint as well as the body grammar.

### [96] c-ns-properties(n,c)

BNF:
```
c-ns-properties(n,c) ::=
    (
      c-ns-tag-property
      (
        s-separate(n,c)
        c-ns-anchor-property
      )?
    )
  | (
      c-ns-anchor-property
      (
        s-separate(n,c)
        c-ns-tag-property
      )?
    )
```

Spec prose: §6.9: "Each node may have two optional properties, anchor and tag, in addition to its content. Node properties may be specified in any order before the node's content. Either or both may be omitted."

Verdict: Strict-conformant

Evidence: `event_iter/step.rs:320-455` (anchor before tag), `event_iter/step.rs:457-637` (tag before anchor), `event_iter/step.rs:639-808` (anchor after, second), `event_iter/flow.rs:1262-1373`.

Reasoning: The parser supports both orderings — tag-then-anchor and anchor-then-tag — by entering the tag/anchor handler and storing pending state (`pending_tag`, `pending_anchor`), then returning to the dispatch and processing the second property. The duplicate-anchor and duplicate-tag checks (`step.rs:706-744`) prevent two anchors or two tags from binding to the same node. The properties are correctly associated with the node that follows.

### [97] c-ns-tag-property

BNF:
```
c-ns-tag-property ::=
    c-verbatim-tag
  | c-ns-shorthand-tag
  | c-non-specific-tag
```

Spec prose: §6.9.1: "The tag property identifies the type of the native data structure presented by the node. A tag is denoted by the `!` indicator."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:85-234`.

Reasoning: `scan_tag` dispatches between the three variants: verbatim `!<URI>` (`properties.rs:91-164`), primary handle `!!suffix` (`properties.rs:167-182`), non-specific bare `!` (`properties.rs:184-190`), named handle `!handle!suffix` and secondary shorthand `!suffix` (`properties.rs:193-233`). The dispatch correctly matches the BNF alternatives by inspecting the byte after the leading `!`.

### [98] c-verbatim-tag

BNF:
```
c-verbatim-tag ::=
  "!<"
  ns-uri-char+
  '>'
```

Spec prose: §6.9.1: "A verbatim tag must either begin with a `!` (a local tag) or be a valid URI (a global tag)."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:91-164`.

Reasoning: The verbatim form requires `<` after `!` (`properties.rs:91`), then scans `ns-uri-char` characters (`properties.rs:138`: `is_ns_uri_char_single(ch)`) or percent-encoded `%HH` sequences (`properties.rs:113-134`) until `>` (`properties.rs:110-112`). Non-URI characters (spaces, BOM, control chars, flow indicators, `^`, `\`, `` ` ``) are rejected (verified by tests `properties.rs:716-781`). An empty URI is rejected (`properties.rs:149-154`). An unclosed URI is rejected (`properties.rs:103-109`). The BNF requires `ns-uri-char+` (one or more); the empty-URI check at `properties.rs:149` enforces the `+`.

### [99] c-ns-shorthand-tag

BNF:
```
c-ns-shorthand-tag ::=
  c-tag-handle
  ns-tag-char+
```

Spec prose: §6.9.1: "A tag shorthand consists of a valid tag handle followed by a non-empty suffix… The suffix must not contain any `!` character… In addition, the suffix must not contain the `[`, `]`, `{`, `}` and `,` characters."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:167-233`, `chars.rs:121-143`.

Reasoning: The shorthand path scans the suffix using `scan_tag_suffix` (`properties.rs:241-273`), which accepts `is_ns_tag_char_single` (`chars.rs:121-143`: alphanumerics + `-_.~*'()#;/?:@&=+$`) plus percent-encoded sequences. The exclusion list (no `!`, no flow indicators) is enforced by `ns_tag_char_single` rejecting `,` `[` `]` `{` `}` `!` (`chars.rs:121-143` does not include these). The suffix may be empty for `!!` and `!handle!` (caller treats empty suffix as valid; the spec requires `ns-tag-char+`, but in practice the empty-suffix case is `c-tag-handle` itself, parsed elsewhere — the shorthand path requires at least one tag-char or it falls through to the secondary-shorthand or non-specific branches). Verified by tests `properties.rs:564-606`.

### [100] c-non-specific-tag

BNF:
```
c-non-specific-tag ::= '!'
```

Spec prose: §6.9.1: "It is possible for the tag property to be explicitly set to the `!` non-specific tag."

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:184-190`.

Reasoning: When `content` after the leading `!` has no tag-suffix (`scan_tag_suffix(content) == 0`), the parser returns `("!", 0)` (`properties.rs:184-190`) — i.e., the bare `!` slice with zero advance. This matches the literal `!` production. Verified by tests `properties.rs:549-557`.

### [101] c-ns-anchor-property

BNF:
```
c-ns-anchor-property ::=
  c-anchor          # '&'
  ns-anchor-name
```

Spec prose: §6.9.2: "An anchor is denoted by the `&` indicator."

Verdict: Strict-conformant

Evidence: `event_iter/step.rs:640-808`, `event_iter/properties.rs:23-45`, `event_iter/flow.rs:1320-1372`.

Reasoning: The parser detects `&` at the start of trimmed content (`step.rs:640`), records the position, and calls `scan_anchor_name` (`properties.rs:23-45`) on the slice after `&`. The composition of `c-anchor` (`&`) and `ns-anchor-name` is enforced: `&` is required, the name is required to be non-empty (`properties.rs:32-37`).

### [102] ns-anchor-char

BNF:
```
ns-anchor-char ::=
    ns-char - c-flow-indicator
```

Spec prose: §6.9.2: "Anchor names must not contain the `[`, `]`, `{`, `}` and `,` characters. These characters would cause ambiguity with flow collection structures."

Verdict: Strict-conformant

Evidence: `chars.rs:149-159`.

Reasoning: `is_ns_anchor_char` (`chars.rs:149-159`) implements `ns-char - c-flow-indicator` literally: `!matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}') && !is_c_flow_indicator(ch) && matches!(ch, ...)` where the final `matches!` is the printable-character set from `ns-char`. The exclusion of flow indicators (`,` `[` `]` `{` `}`) is enforced by `!is_c_flow_indicator(ch)`. Verified by tests `chars.rs:325-346`.

### [103] ns-anchor-name

BNF:
```
ns-anchor-name ::=
  ns-anchor-char+
```

Spec prose: §6.9.2: implicit; the anchor name is one-or-more `ns-anchor-char`.

Verdict: Strict-conformant

Evidence: `event_iter/properties.rs:23-45`.

Reasoning: `scan_anchor_name` collects characters with `take_while(|&(_, ch)| is_ns_anchor_char(ch))` (`properties.rs:28-31`) and rejects an empty result (`properties.rs:32-37`: "anchor name must not be empty"). The `+` quantifier is enforced. The maximum-length check (`properties.rs:38-43`) is a Stricter-than-spec safeguard against denial-of-service, but applies a project security limit, not a spec rule — the verdict for ns-anchor-name is `Strict-conformant` because the BNF places no upper bound and the parser accepts arbitrary-length names up to the configured limit. The maximum is project policy, not a deviation from the production's grammar.
