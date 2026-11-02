# Feature Log

Feature decisions for rlsp-yaml-parser, newest first. Tiered by
user impact, implementation feasibility, and alignment
with existing infrastructure.

**Tiers:**
- **1** — High impact, feasible now
- **2** — Medium impact, moderate effort
- **3** — Valuable but higher effort
- **4** — Niche or high effort / low return

---

### Properties Keep Their Node [completed]

**Description:** A line of properties and a key at the column of a `?` no longer
loses text. `? &x` followed by `&zz : v` and `: v` loads the entries `&x` with an
empty value, `&zz` with `v`, and an empty key with `v`, as libfyaml 0.9.6 does;
before, `&zz` was dropped and `v` became the value of `&x`. The same holds for a
tag (`? !!str` followed by `!zz : v`) and for a key in a sequence item (`- ? &d`
followed by `  &zz : v`). `?` followed by `&zz : v` is read as an empty entry,
then `&zz` with `v`, where `&zz` was the anchor of the empty key before.

A second anchor or tag for one node reports an error where one of the two was
dropped: `&zz` followed by `&a4 !!map` (the first was dropped) or `&a4 !!str x`
(the second), `a: &zz` followed by `  &a4 !!map`, `a: &a !t` followed by
`  &b zz`, and `a: !t &a` followed by `  !zz` (also after `---`). The same holds
for `&p1` followed by `&q1 [y]`, `&q1 {y: z}`, `&q1 |` or `&q1 >`, and for `&p1`
followed by `!q2 &q1 [y]`: they loaded with `&p1` and without `&q1`. `&zz`
followed by `&a4 !!str k: v` and `&a4 !!map` followed by `&a5 !!str key5: value4`
keep loading: the second properties belong to the key.

The properties above a flow-collection key that has properties of its own now
belong to its mapping, as above a plain key: `!t &a` followed by `!zz [y]: v` is
the mapping `&a !t` with the key `!zz [y]`, as in libfyaml, and so are `&p1`
followed by `!q1 [y]: v`, which put both on the key before. Above a
flow-collection key without properties of its own (`&a` followed by `[a]: b`)
they stay on the key; that is open on `.ai/backlog.md`. `format_yaml` leaves a
file with such an error unchanged.
**Complexity:** Low
**Comment:** YAML 1.2.2 [96] `c-ns-properties` (a node has at most one anchor and
one tag), [190] `c-l-block-map-explicit-key` and [192]
`ns-l-block-map-implicit-entry` (the key of a `?` entry holds only its own
properties, and the entry after it starts at the mapping's column). libfyaml 0.9.6
reports "missing document start", "two anchors for a single value" or "did not find
expected key" for the errors and reads the entries as above. Inputs that fed this
entry are in `rlsp-yaml-parser/tests/conformance/libfyaml_events.rs`.
**Tier:** 1

---

### `load` Reports an Error for a Second Root Node [completed]

**Description:** `load` ends with an error when the event stream holds a node
after the root of a document, or outside any document, where it used to skip the
node and drop its text. A pending anchor followed by an alias on the next line is
such an input: `&k` then `*a`, `--- &k` then `*a`, `---` then `&k` then `*a`,
`&k # c` then `*a`, and `x: &k` then `*a` (the alias at the root column or
indented) now report an error; they loaded before as the alias alone, without
the anchor. An anchored empty item or value followed by an alias as an entry of its own
(`- &k` then `- *a`, `x: &k` then `y: *a`) loads as before. `parse_events` still
yields the same events for these inputs, and the error carries the position of
the empty scalar after the alias, which can lie on the line after the last one.
`format_yaml` leaves a file with such an input unchanged.
**Complexity:** Low
**Comment:** YAML 1.2.2 [207] `l-bare-document` (one block node per document) and
[104] `c-ns-alias-node` (§7.1: an alias node does not specify properties). libfyaml
0.9.6 reports "unexpected alias" at the alias for all of these inputs. The
parser still accepts `- &k` then `  *a` (an alias with a pending anchor inside a
collection) and loads it as two items.
**Tier:** 1

---

### A Plain Scalar in a Flow Collection Keeps Its Blank Lines and Ends at a Comment [completed]

**Description:** A plain scalar that continues over blank lines in a flow
collection keeps them as line feeds: `[a`, a blank line, `  b]` loads the value
`a\nb` (two blank lines give two line feeds in between), where it loaded `a b`
before. This holds for items of flow sequences, for keys, and for the value of a
single pair in a flow sequence (`[a: b`, a blank line, ` c]`), also inside block
collections. A value in a flow mapping does not continue over lines: `{a: b`, a
blank line, ` c}` reports an error, as it did before. A comment ends the plain scalar before it: in
`[a # c` followed by `  b]`, and in `{a # c` followed by ` b: 1}`, `b` is an entry
without the `,` before it, and the parser reports "missing comma in flow
sequence" or "in flow mapping", as libfyaml 0.9.6 does, at the same position.
Before, it loaded `[a]` and `{a: 1}` and dropped the text after the comment
without an error, so `format_yaml` deleted it from the file; now `format_yaml`
leaves such a file unchanged, and writes the value `a\nb` of the first form as
`["a\nb"]`. The same error now reads "missing comma in flow sequence" (or "in flow
mapping") for a plain scalar after a complete entry (`[ "q" b ]`), where it read
"missing comma between flow sequence entries". A plain scalar that spans lines in
a flow collection is now limited to `MAX_SCALAR_LEN` and is built in linear time.
**Complexity:** Low
**Comment:** YAML 1.2.2 [71] `b-l-trimmed`, [72] `b-as-space`, [74] `s-flow-folded`
and [135] `ns-plain-multi-line`: no comment sits between the lines of a plain
scalar, and n blank lines fold to n line feeds. Other inputs where the parser and
libfyaml still differ on a plain scalar over several lines in a flow collection
(`{k: v` then ` w}`, `[a` then ` "b"]`) are not covered by this entry.
**Tier:** 1

---

### A Bare `-` Looks Past Comment and Blank Lines [completed]

**Description:** The item of a bare `-` is its value when that value starts on the
first line below the dash that is neither blank nor a comment, and that line is indented
deeper than the dash; the comment and blank lines before it may have any indent.
`-` then `# c` then `  b` is one item `b`, with `# c` as its leading comment; it used to
be an empty item followed by a second item `b`. So are `-`, a blank line, `  b`, and
`-`, `  # c`, `  - y` (a nested sequence). `-` then `  # c` then `- x`, and `-` then
`  # d` at the end of the input, are an empty item (and `x`): the empty item used to be
lost, as the indented comment looked like the item's content. `-` then `# about x` then
`- x` still keeps its comment with `x`. Every comment reaches the output of
`format_yaml`, and formatting twice changes nothing. `parse_events` and `load` give the
events of libfyaml 0.9.6 on these inputs.
**Complexity:** Low
**Comment:** YAML 1.2.2 [185] `s-l+block-indented` (`e-node s-l-comments`), [78]
`l-comment`, [79] `s-l-comments`.
**Tier:** 1

---

### A Key Reads Its Own `:` [completed]

**Description:** A `:` line after a key with an empty value is an entry of its own with
an empty key, not the value of the key before it. `a:` followed by `: c` is two entries,
`a` with an empty value and an empty key with the value `c`; so are `:` then `: c`,
`?` then `:` then `: c`, `? a` then `:` then `: c`, `a: 1`, `[x]:`, `: v`, and `a: &y`
then `&k : v` (`&y` stays on the empty value of `a`, `&k` goes to the empty key).
A `:` line that is indented deeper than the key opens a nested mapping with an empty
key: `x:` then `  : v` is `x` with the value `{"": v}`, and `? a` then `:` then `  : v`
is `a` with the value `{"": v}`. Before, the `:` line was read as the indicator of the
key before it, so `c` or `v` became that key's value, the empty value was lost, and an
anchor on the second key was dropped; `format_yaml` then wrote the wrong pairing.
Aliases and flow collections as keys (`*a : v`, `[x]: v`, `&k [x]: v`) load as before.
A `:` line that no key can own (`*a` then `: v`, `a: 1` then `  : v`) is still an error.
`parse_events` and `load` give the events of libfyaml 0.9.6 on these inputs, with one
class of exceptions: a block collection on the line of an empty key's `:` (`a:` then
`: - b`, `[x]:` then `: - a`, `a:` then `: b: c`, `a:` then `: : v`) and a property line
below a nested empty-key entry (`*a :`, ` :`, ` &y`) are syntax errors here, where
libfyaml reads them and the parser used to load them with the wrong pairing (`a:` then
`: - b` was `{a: [b]}`). The class is in `docs/conformance/design-decisions.md`; the
user has not decided it yet.
**Complexity:** Low
**Comment:** YAML 1.2.2 [191] `l-block-map-explicit-value`, [192]
`ns-l-block-map-implicit-entry`, [194] `c-l-block-map-implicit-value`, [96]
`c-ns-properties`, [106] `e-node`, [154] `ns-s-implicit-yaml-key`.
**Tier:** 1

---

### A `%` Line Inside a Document Is an Error [completed]

**Description:** A line that starts with `%` below `---` is a syntax error: `---`
then `%a` (with or without a node after it), `---`, `!t`, `%a`, `a:` then `%b`,
`a: 1` then `%b`, `--- # c` then `%a`, and `---`, `%a`, `---`, `x`; so is `--- x # c`
then `%y`. Before, the parser skipped such a line without a word in most of these
places, and `format_yaml` deleted it from the file. A directive is valid before
`---` only, as in `%YAML 1.2` then `---`, or after `...`. A `%` line stays content
inside a block scalar (`--- |` then `%a`), in a quoted scalar, and as a continuation
line of a plain scalar (`--- x` then `%y` is the scalar `x %y`). libfyaml 0.9.6 skips
the `%` line with a warning; the parser reports an error instead, so that no text
vanishes (see `docs/conformance/design-decisions.md`). A line the block parser does
not recognise, such as an indented control character, is an error as well, where an
indented one was skipped before.
**Complexity:** Low
**Comment:** YAML 1.2.2 [126] `ns-plain-first`, [22] `c-indicator`, [82]
`l-directive`, [209] `l-directive-document`, [211] `l-yaml-stream`.
**Tier:** 1

---

### A Plain Scalar After `---` Continues Over the Lines Below [completed]

**Description:** `--- x` followed by `  y` loads as the one scalar `x y`, as does
`y` at column 0, `%y`, `@y` or `- y`; and `  x` followed by `y` at column 0 loads
as `x y`. Before, the lines below `--- x` were dropped or read as a second
node: `y` was a second root scalar, `  y: 1` turned `x` into a key, `%y` was
skipped and `  @y` was an error, and `format_yaml` deleted that text from the
file. These inputs now report a syntax error, as in libfyaml 0.9.6: a comment
line ends the scalar and text below it is a second root node (`--- v`, `# d`,
`w`), a key line below it (`--- x`, `y: 1`), a key on the marker line
(`--- x: 1`), `-`, `?` or `:` followed by a space or the end of the line on the
marker line (`--- - a`, `--- ? a`, `--- -`, `--- ?`; they loaded as the plain
scalar `"- a"`), and plain text at column 0 under a block mapping key (`a:`,
`w`, `x`; the two lines were merged into the value `w x`). `--- x # y: z` loads
the scalar `x` with its comment. These inputs give the same events and errors as
libfyaml 0.9.6; `--- x # c` followed by `%y` is an error here, where libfyaml
skips the `%y` line. `parse_events()` and `load()` give the same result.
**Complexity:** Medium
**Comment:** YAML 1.2.2 [207] `l-bare-document`, [197] `s-l+flow-in-block`,
[134] and [135] `ns-plain-multi-line`, [126] `ns-plain-first`. The root node has
no parent indent, so its continuation lines may start at any column. A `%YAML`
line below such a scalar is part of it, as in a bare document.
**Tier:** 1

---

### Properties Alone Are an Empty Node [completed]

**Description:** A tag or an anchor with nothing after it is an empty node that
carries it, wherever it stands: above `---` or `...` (`&x` then `---` is a
document whose root is empty and anchored), before a `,`, `]` or `}` in a flow
collection (`[ !!str ]`, `{ k: &a }`, `{ !t , b: 1 }`), and as the last item of a
sequence at its key's column (`k:\n- !!str\nb: 1`). Before, these properties
were dropped, or the next key took them. Events equal those of libfyaml 0.9.6.
**Complexity:** Low
**Comment:** YAML 1.2.2 [96] `c-ns-properties` on an empty node, [105] `e-scalar`.
**Tier:** 1

---

### Properties and `?` in a Flow Sequence Pair [completed]

**Description:** In `[ !!str : a ]`, `[ &x : a ]` and `[ !t &x : b ]` the
properties belong to the empty key of the single pair. Before, the key took none
of them: a core tag was dropped and any other property moved to the value.
`[ ? ]`, `[ ? a ]` and `[ ? !!str , b ]` are single pairs with an empty key or
value; before, `?` was dropped and the entry became a plain scalar. `[ !t ? ]` and
`[ ? ? ]` are errors. Events equal those of libfyaml 0.9.6.
**Complexity:** Low
**Comment:** YAML 1.2.2 [150] `ns-flow-pair` and [151] `ns-flow-pair-entry`.
**Tier:** 1

---

### A Flow Entry Without the Comma Before It Is an Error [completed]

**Description:** `[: :]`, `[a: b: c]`, `{: :}`, and a collection that starts right
after an entry (`[ a [ ] ]`, `{ a: b { } }`) report "missing comma in flow
sequence" or "in flow mapping", as libfyaml 0.9.6 does, at the same position.
Before, the parser accepted them; `[: :]` gave an unbalanced event stream, and
`load()` on such a file never returned and used memory until the process died.
`load()` now also ends in an error for any event stream that closes a collection
it did not open or stops inside one.
**Complexity:** Low
**Comment:** YAML 1.2.2 [138], [141] and [150]: another entry needs a `,`. The
check in the loader holds whatever the parser emits.
**Tier:** 1

---

### Only `~` Alone Is Null [completed]

**Description:** A plain scalar that starts with `~` but is not `~` alone
(`~/bin`, `~x`) loads as a string. Before, it got the null tag.
**Complexity:** Low
**Comment:** YAML 1.2.2 §10.3.2, the Core schema null regex.
**Tier:** 1

---

### Tabs After the Indentation [completed]

**Description:** A tab between the indentation and a tag, anchor or alias
(`-\n  \t!t a`) separates them and is no part of the tag or the anchor name;
before, the tab became part of the tag (`"\t!t"`). A `-`, `?` or mapping key
after indentation spaces and a tab (`k:\n  \ta: 1`) is an error; before, the
parser read the entry with the tab cut off and lost the lines under it. libfyaml
0.9.6 rejects the first kind too (see `docs/conformance/design-decisions.md`).
**Complexity:** Low
**Comment:** YAML 1.2.2 [69] and [66] allow the tab; [183] and [187] need spaces
for the indentation of a block entry.
**Tier:** 1

---

### A Plain Scalar Cannot Start With a Reserved Indicator [completed]

**Description:** A node that starts with `,`, `@`, a backtick, or `%` is a syntax
error: `v: ,`, `- @a`, `` `a: b ``, `a: %b`, `--- ,`, `--- %a`.
Before, the parser took the line for an empty node and dropped its text, so
a file with such a line loaded with that data missing. A file that holds one
now reports an error at the indicator; libfyaml 0.9.6 rejects them too. The same
characters inside a scalar, in a continuation line, in a flow collection after
the first character, in a quoted scalar, or in a comment are content as before.
A `%` at the start of a line opens a directive only before `---`.
**Complexity:** Low
**Comment:** YAML 1.2.2 [126] `ns-plain-first` excludes the indicators from
the first character of a plain scalar. The yaml-test-suite and the real files of
the repository load as before.
**Tier:** 1

---

### An Explicit Flow Key Keeps Its Exemption From the Key Length Limit [completed]

**Description:** An explicit key in a flow collection (`? K`) has no length limit.
Before, a `:` inside the key ended the exemption, so `{ ? {a: 1, b: <1100
characters>}: v }` was rejected with "implicit flow key exceeds 1024 Unicode
characters". It loads now, as it does in libfyaml 0.9.6. Implicit keys keep the
limit.
**Complexity:** Low
**Comment:** The parser tracked the current key in one variable for all open flow
collections; each open collection now has its own.
**Tier:** 1

---

### A Flow Collection or an Alias After `---` Is the Root Node [completed]

**Description:** `--- [alpha, bravo]` loads as a sequence of two scalars,
`--- {a: 1}` as a mapping, and `--- *a` as an alias. Before, `--- [alpha,
bravo]` loaded as the plain scalar `"[alpha, bravo]"`, `--- *a` as the scalar
`"*a"`, and `--- {a: 1}` was an error. `parse_events()` and `load()` give the
same events as libfyaml 0.9.6. A collection written with the bare `!`
(`- ! [a, b]`) keeps the span of the `!` in `tag_loc`, as a scalar does, so a
consumer can tell it from a collection without a tag; its tag is still `!!seq`
or `!!map`. Content after the node on the `---` line is an error: `--- [x] [y]`
(it loaded as one scalar), `--- [x]: 1`, `--- {a: 1}: 2`, `--- *a b`,
`--- *a: v` and `--- *a : v`, since a block mapping cannot start on the marker
line and a document holds one node. A closing bracket there (`--- ]`, `--- }x`)
is an error too, as on the line below; it loaded as a plain scalar. A document
that holds such a line loads
differently than before: a formatter that rewrites `--- [a]` as `---`, then
`[a]` on the next line changed its data until now.
**Complexity:** Low
**Comment:** The inline-content path of the `---` line took any text that was
not a block scalar header, anchor, tag or quote for a plain scalar. A flow
collection or alias there now goes through the dispatch of a later line.
**Tier:** 1

---

### A Comment Stays With the Node It Is Next To [completed]

**Description:** `load()` stored some comments on the wrong node. A comment on
the dash line of a later item (`- a: 1`, then `- # d` and `  b: 2`) or on the `?`
line of a later key was the previous collection's or empty value's trailing
comment; it is now a leading comment of that item or key. A comment after a
quoted scalar that spans several lines (`a: "x`, `  y" # c`) was stored on the
next entry; it is now the scalar's trailing comment. A comment line below a tag
or an anchor with no node (`- !!str`, then `# d` and `- x`) was a leading
comment of that empty item; it now belongs to what follows (`x`), or to the
document's end comments when nothing follows. A comment on the properties' own
line (`- !!str # c`) and a comment before a node that continues on a later line
stay with the node. The event stream is unchanged.
**Complexity:** Medium
**Comment:** The loader read the line of a node from the end of its span. A block
collection's span ends at the start of the next line and the span of an empty
scalar sits where the next token starts, so the comment of the next line looked
like the node's own. Without this, a formatter that prints a comment above the
next item reads it back on the item before it, and formatting twice gives a
different text.
**Tier:** 1

---

### Every Comment Is an Event [completed]

**Description:** `parse_events()` emits an `Event::Comment` for every comment,
in source order, not only for comments on their own line or after a plain
scalar. New: a comment after a quoted scalar (`a: "q" # c`), after `k:`, `-`,
`?` and `:` with no value on the line, after an anchor or tag, on a block
scalar's header line (`a: | # c`), on the last line of a multi-line plain
scalar, after a scalar on the `---` line (`--- v # c`), after a directive
(`%YAML 1.2 # c`, `%TAG ... # c`, reserved directives), and after an orphan
`...`. Each `Comment` span starts at its `#`.
Comments are now validated at these positions: a comment over
`MAX_COMMENT_LEN` (4096 bytes), or one with a character outside c-printable,
is an error everywhere, also after a plain scalar in a sequence or mapping
value, after `---` and `...`, and inside a flow collection. Before, some of
these were accepted unchecked, so a document with such a comment that loaded
now fails with `InvalidCharacter` or a syntax error at the `#`; libfyaml
accepts both (see `docs/conformance/design-decisions.md`, [75]).
**Complexity:** Medium
**Comment:** A comment that is dropped from the event stream cannot reach the
tree or the formatter, so this is the first step of keeping every comment.
The span of a token, and the position of an
error, after the closing quote of a multi-line double-quoted scalar is now its
real offset, in flow and block context (it was short by the closing line's
indentation).
**Tier:** 1

### Comment Line Ends a Plain Scalar [completed]

**Description:** A comment line after a plain scalar is a comment, not part
of the value. `port: 8080` followed by `  # default` loads as `8080` (int)
instead of the string `"8080 # default"`, and the same holds for sequence
items, explicit keys, and a root scalar, with the comment indented, after a
blank line, or after a tab. A plain-scalar continuation after the comment
line is a syntax error that points at the line after the comment
(`key: word1`, `  # xxx`, `  word2`), as in libfyaml. `load()` also keeps
`explicit_end` for `v`, `# d`, `...` and for `v # c`, `...`: a comment after
the root node no longer hides the `...`.
**Complexity:** Low
**Comment:** The comment line was folded into the value before, so documents
with an indented comment under a plain value changed their data; the events
and the tree now match libfyaml 0.9.6. The formatter no longer duplicates the
comment and writes it on its own line. A comment on the last continuation line
(`a:`, `  v`, `  w # c`) and comments before a later `---` or `...` are not
covered by this entry.
**Tier:** 1

### Alias and Flow Keys After a Finished Entry [completed]

**Description:** A block mapping key that starts with an alias or a flow
collection (`*y : v`, `[x]: v`, `{x: 1}: v`) is now the next entry of the
mapping that holds the entry before it. Previously `a: 1` followed by
`[x]: 2` loaded the second key inside an extra nested mapping, and an alias
key after a key without a value (`a:` followed by `*y : 2`) was taken for
the value of `a`. The same holds after an explicit `?` key, after a
sequence at the mapping's column (`a:` followed by `- b` and `[x]: 1`), and
for a key left without a value (it gets an empty value, as for a plain key). A
`!!tag` or `&anchor` on the line after a key without a value (`a: !!str`
followed by a key) is now that empty value's, not the next key's.
**Complexity:** Medium
**Comment:** The key forms were accepted before; only the resulting
events and tree change. For the forms above the events match libfyaml
0.9.6. A `!!tag` line before an alias key that opens a mapping
(`!!map` followed by `*y : v`) is the mapping's tag instead of an error.
An alias that is the whole root node (`*a`) now ends the document's
content: `*a` followed by `b: 1` is an error instead of two root nodes.
Two anchor lines in a row (`&y` then `&k`, each alone on its line) are an
error too instead of the second anchor silently replacing the first. An
anchor or tag that the next property displaced (`a: &y !!str`) now stays
on its own node instead of landing on a later, unrelated one, and a
property line before `---` or `...` no longer carries into the next
document.
**Tier:** 1

### Content After a Complete Block Node [completed]

**Description:** A block sequence item, a block mapping value, or an
explicit `?` key holds exactly one node. A flow collection, alias,
anchored or tagged line, or other node that starts after that node is
complete — indented past the collection, between two levels, at the `-`
column, or at a mapping's column without being a key — is now a syntax
error that points at the stray line. Previously such a line was read as a second node
(`- "q"` followed by `  w` loaded as two items), silently changing the
data. Plain-scalar continuation lines (`- v` followed by `  w` loads as
`"v w"`) and a value on the line after a bare `-` or `key:` are not
affected. A sequence entry after a complete mapping value or explicit key, and a
second node on the same line after a flow collection or alias
(`- [x] [y]`, `a: *b w`), are errors as well. A blank line inside a plain-scalar sequence item now folds
into the item like it does for a mapping value (`- a`, a blank line,
`  b` is one item `"a\nb"`); it used to load as two items.
A single-quoted scalar followed by more content on the same line
(`- 'q' w`) now reports the same error as the double-quoted form instead
of dropping the rest of the line. A property line (`&x`, `!!str`) on its own
line after a complete node (`a: 1`, `&x`, `b: 2`) is an error too: the
properties of an implicit key belong on the key's own line. Content
after an indented root collection at a smaller column (`  - a`, then `baz`
at column 0, or `  a: 1` then `b: 2`) is a second root node and reports
the same error as after a root collection at column 0. A quoted implicit
key followed by more text before its `:` (`'k' x: v`) is an error instead
of silently losing the text.
**Complexity:** Medium
**Comment:** Documents that relied on the old leniency now fail to load.
The error is reported through both `parse_events()` and `load()`. Alias
names keep the YAML 1.2 alphabet, so `*a: v` is an error because `:`
belongs to the name; write `*a : v` (with a space) for an alias key.
**Tier:** 1

### Literal Stream Character Validation [completed]

**Description:** The parser now enforces the YAML 1.2 §5.1 two-tier character-set
rule on all literal stream content. Non-printable characters in the input produce
a parse error naming the offending codepoint in `U+XXXX` format:

- **c-printable rule** (stream-wide): applied to plain scalar content, block scalar
  content (literal `|` and folded `>`), and comment bodies. Rejects C0 controls
  (except TAB), DEL (U+007F), C1 controls (except NEL U+0085), and non-characters
  U+FFFE/U+FFFF.
- **nb-json rule** (quoted scalars): the spec requires that YAML processors allow
  all non-C0 characters inside quoted scalars (§5.1 JSON-compatibility clause).
  Single-quoted and double-quoted scalar content may contain DEL, C1 controls, and
  U+FFFE/U+FFFF — only C0 controls (except TAB) are rejected.

Previously the parser accepted any byte that cleared its delimiter scan, silently
passing non-printable bytes to callers. Inputs such as a BEL character in a plain
scalar or a SOH byte in a comment now produce structured parse errors.

Existing specific checks (NUL in trailing comments, BOM in document body) are
preserved and still fire with their targeted messages; the new checks are backstops
for any other non-printable that reaches a scanner.
**Complexity:** Low
**Comment:** Security hardening: passing raw non-printable bytes through a YAML
parser into downstream log pipelines, LSP diagnostics, and rendered output is a
risk. The fix is strict-reject (not warn or diagnostic) — the user's stated
preference and the spec's normative wording ("YAML streams use only the printable
subset"). The nb-json exception for quoted scalars is a spec mandate ("must allow"),
not optional. The enforcement adds a second byte-scan pass over content slices;
overhead on valid YAML (which contains no non-printables) is near-zero.
**Tier:** 1

---

### LoadError Position Fields [completed]

**Description:** Five `LoadError` variants (`NestingDepthLimitExceeded`,
`AnchorCountLimitExceeded`, `AliasExpansionLimitExceeded`, `CircularAlias`,
`UndefinedAlias`) now carry a `pos: Pos` field that identifies the source
position of the node that triggered the error. LSP diagnostics produced from
these errors now point to the offending node rather than line 0, column 0.
**Complexity:** Low
**Comment:** Previously these variants reported `Pos::ORIGIN` in the LSP
diagnostic range, sending users to the start of the file regardless of where
the error occurred. The position is sourced from existing span tracking —
no new parser state was required.
**Tier:** 1

---

### Single-comparison document dispatch [completed]

**Description:** The core parse loop's per-line handler selection now performs a
single byte comparison instead of up to 15 sequential checks. Each YAML structural
indicator (`-`, `*`, `!`, `&`, `[`, `{`, etc.) has a unique first non-whitespace
byte; the parser matches on that byte directly and routes to the correct handler in
one step. Mapping key detection (which can start with any byte) runs once
unconditionally after the byte match.
**Complexity:** Low
**Comment:** Dense and small documents (Kubernetes manifests, short configuration
files) spend a measurable fraction of parse time in handler selection. The
restructure eliminates redundant probes without changing any parse semantics —
all 726 yaml-test-suite conformance tests continue to pass.
**Tier:** 1

---

### Lazy Position Resolution via `LineIndex` [completed]

**Description:** `Span` now stores only byte offsets (`start: u32`, `end: u32`,
8 bytes total, down from 48 bytes). Line and column numbers are resolved on demand
via the new `LineIndex` type. Each `Document<Span>` exposes a `line_index()` accessor
that returns a `&LineIndex` shared across all documents in the same parse. Callers
convert byte offsets to `(line, column)` pairs with `idx.line_column(offset)`.
**Complexity:** Medium
**Comment:** `Span` is the most-allocated type in the parser — every AST node and
event carries one. Shrinking it from 48 to 8 bytes reduces peak heap usage and
improves cache locality for batch parsing workloads. The `Pos` type is retained
for error reporting (`LoadError::Parse.pos`). The `LineIndex` is built
once per input string and shared via `Arc` across multi-document streams so the
newline table is not duplicated.
**Tier:** 1

### Named Tag Handle `_` Rejection [completed]

**Description:** Named `%TAG` directive handle names now reject `_` per YAML 1.2.2
§5.6 (production [38] `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`) and
§6.8.1 (production [92] `c-named-tag-handle ::= c-tag ns-word-char+ c-tag`). A
`%TAG` line such as `%TAG !my_handle! tag:example.org,2024:` is now a parse error.
Only `%TAG` directive handle names are affected — inline tag suffixes (e.g.,
`!!my_type`) continue to accept `_` because `ns-uri-char` (production [39])
explicitly permits it.
**Complexity:** Low
**Comment:** Previously the parser accepted `_` in named handle names, silently
diverging from the spec alphabet. The fix aligns the `is_valid_tag_handle` predicate
with `ns-word-char` exactly.
**Tier:** 1

### Flow Collections (`[...]` and `{...}`) [completed]

**Description:** Parse flow sequences and flow mappings. The parser
uses an explicit `Vec<FlowFrame>` stack — no recursion — so deeply
nested flow input cannot overflow the call stack. Flow and block
nesting depths share the same `MAX_COLLECTION_DEPTH` counter so
combined depth is bounded uniformly.
**Complexity:** High
**Comment:** The largest single method in the codebase. Non-recursive
explicit stack was a deliberate security decision: recursive descent
over untrusted flow input would be exploitable via deep nesting.
**Tier:** 1

### Comment Preservation [completed]

**Description:** Comments are emitted as `Event::Comment` events with
their body text, and `load()` stores every one of them in the AST, once.
Leading comments (comment lines preceding a node) and trailing comments
(the comment on the line of a node, including a key's own line and a
block scalar's header line) are attached to AST nodes. The comments
around a document are in four `Document` fields: `prefix_comments`
(before `---` or the implicit content, including directive lines),
`comments` (after `---`, before the root), `end_comments` (after the
root, before `...`, `---` or the end of input) and `after_end_comments`
(after the last `...`).
**Complexity:** Medium
**Comment:** Comments are fully first-class in the AST. Leading
comments are attached to mapping entries and sequence items;
trailing comments are attached to keys, values and items. The spec
does not define comment ownership outside a node (YAML 1.2.2 §6.6),
so the document fields follow the order of the
source. A stream that holds only comments loads as zero documents and
has no place for them.
**Tier:** 1

### AST Loader — Lossless Mode [completed]

**Description:** The `loader` module converts the event stream into
a `Vec<Document<Span>>`. In lossless mode (the default), alias
references are preserved as `Node::Alias` nodes rather than expanded.
This is the safe default for LSP use: no expansion budget is needed,
and alias bombs cannot cause memory exhaustion.
**Complexity:** Medium
**Comment:** Lossless mode is safe for untrusted input without any
alias expansion limit because no expansion ever occurs.
**Tier:** 1

### AST Loader — Resolved Mode [completed]

**Description:** An opt-in loader mode that expands aliases inline.
The loader tracks a total expanded-node counter guarded by
`max_expanded_nodes` (default 1 000 000) to prevent Billion Laughs
alias bombs. Circular aliases are detected via an `in_progress` set
and returned as `LoadError::CircularAlias`. A node produced by an alias
carries none of the anchor's comments: the definition site keeps them, and
the comments of the alias site (`a: *x # c`) stay on the expanded node.
**Complexity:** Medium
**Comment:** Resolved mode is needed by tools that want a fully
materialized document. The expansion limit and cycle detection are
defence-in-depth; the primary recommendation for untrusted input is
lossless mode.
**Tier:** 1

### Security Limits [completed]

**Description:** Eight compile-time constants (in `src/limits.rs`) cap
inputs from untrusted sources. All limits return structured
`Error`/`LoadError` values — never panics:
- `MAX_COLLECTION_DEPTH` (512) — combined block + flow nesting depth;
  unified to prevent bypass by mixing sequence and mapping nesting.
- `MAX_ANCHOR_NAME_BYTES` (1 024) — anchor and alias name scanning;
  prevents CPU exhaustion on degenerate long names.
- `MAX_TAG_LEN` (4 096) — raw scanned tag (verbatim URI or suffix).
- `MAX_COMMENT_LEN` (4 096) — per-line comment body scanning.
- `MAX_DIRECTIVES_PER_DOC` (64) — `%YAML` + `%TAG` directives per
  document; prevents HashMap exhaustion.
- `MAX_TAG_HANDLE_BYTES` (256) — `%TAG` handle length.
- `MAX_RESOLVED_TAG_LEN` (4 096) — fully-resolved tag string after
  `%TAG` prefix expansion; prevents allocation of oversized resolved
  strings.
- `MAX_SCALAR_LEN` (1 048 576 = 1 MiB) — quoted scalar length for both
  single-quoted and double-quoted styles; applied uniformly on the
  borrow path (no escapes), the escape-decode (owned) path, and
  accumulated multi-line length; it also bounds a plain scalar that
  spans lines in a flow collection; prevents DoS via unbounded scalar
  allocation.
**Complexity:** Low
**Comment:** All limits are generous for real-world YAML (Kubernetes
documents rarely exceed 20 levels deep; tags are under 30 bytes) while
bounding worst-case CPU and memory usage.
**Tier:** 1

### Hex Escape Security Hardening [completed]

**Description:** The double-quoted scalar lexer applies two security checks to
hex escapes (`\x`, `\u`, `\U`) that go beyond what YAML 1.2.2 §5.7 requires:
(1) `quoted.rs:594-606` — the decoded character must be a `c-printable`
codepoint; non-printable hex-escape sequences are rejected with a parse error;
(2) `quoted.rs:608-618` — hex escapes that decode to a bidi-override character
(U+202A–U+202E, U+2066–U+2069) are rejected. Named escapes (`\0`, `\a`, `\b`,
`\e`, `\N`, etc.) are intentionally exempt from both checks — they produce
well-known control characters whose semantics are unambiguous. This is a
deliberate divergence from the spec, recorded as `Strict (security-hardened)`
in `docs/conformance/bnf-§5.md` entries [59]–[61].
**Complexity:** Low
**Comment:** The spec permits any codepoint via hex escapes, but accepting
arbitrary non-printable or bidi-override codepoints through a YAML file is a
security risk in LSP and pipeline contexts. Named escapes are exempt because
their output is predictable; hex escapes are not.
**Tier:** 1

### Implicit Mapping Key Length Limit [completed]

**Description:** Implicit mapping keys (those without a leading `?` indicator) are
capped at 1024 Unicode characters in both flow context (YAML 1.2 §7.4.3) and block
context (§8.2.2). A key whose `:` value indicator appears more than 1024 characters
from the key start is rejected with a parse error. Explicit `?`-introduced keys are
not subject to this limit.
**Complexity:** Low
**Comment:** The spec mandates this limit to bound parser lookahead. Enforcement
closes four previously-Lenient conformance entries ([154], [155], [192], [193]) and
brings the parser to full conformance on this point. Only implicit keys are affected;
explicit key content remains unrestricted.
**Tier:** 1

### Encoding Detection and Decoding [completed]

**Description:** The `encoding` module implements YAML 1.2 §5.2
encoding detection. Detects UTF-8, UTF-16 LE/BE, and UTF-32 LE/BE
via BOM and null-byte heuristic. Decodes any supported encoding to
UTF-8 and strips the BOM at stream start. Normalizes CRLF and lone CR
to LF. BOM is also accepted (stripped) at document-prefix positions
within a multi-document stream, implementing the `c-byte-order-mark?`
component of `l-document-prefix` (§9.1.1) via `signal_document_boundary()`
in `lines.rs`.
**Complexity:** Medium
**Comment:** UTF-32 BOM detection precedes UTF-16 because the UTF-32
LE BOM (`FF FE 00 00`) is a prefix of the UTF-16 LE BOM (`FF FE`).
**Tier:** 1

### Span Tracking [completed]

**Description:** Every event carries a `Span` covering the source
bytes that produced it — `start` and `end` are both `Pos` values
(byte offset, line, column). Zero-width spans mark synthetic events
(e.g. `StreamStart`). The loader propagates spans into the AST so
every `Node` carries source location for LSP diagnostics.
**Complexity:** Low
**Comment:** Accurate spans are essential for LSP use. The parser
tracks both byte offsets (for range operations) and line/column
(for LSP `Position` types) in a single `Pos` struct to avoid
redundant re-scanning.
**Tier:** 1

### Streaming Event API [completed]

**Description:** `parse_events(input)` returns a lazy
`Iterator<Item = Result<(Event, Span), Error>>` that produces
events on demand. First-event latency is O(1) — the caller receives
`StreamStart` before any bulk processing. The iterator is zero-copy
for most scalars: `Event::Scalar.value` is a `Cow::Borrowed(&str)`
that slices directly from input when no transformation is needed.
**Complexity:** Medium
**Comment:** The streaming design is a fundamental architectural
decision. It allows the LSP to begin processing before the full
document is parsed and avoids materializing an intermediate
representation when the caller only needs events.
**Tier:** 1

### Anchors and Aliases [completed]

**Description:** `&name` anchor definitions and `*name` alias
references are scanned and included in the respective events as
`anchor: Option<&str>` and `Event::Alias { name }`. The loader
builds an anchor map and resolves aliases in resolved mode or
preserves them as `Node::Alias` in lossless mode.
**Complexity:** Medium
**Comment:** Zero-copy: anchor and alias names borrow directly from
input without allocation.
**Tier:** 1

### Tag Resolution [completed]

**Description:** All four tag forms are recognized and resolved at
parse time: verbatim (`!<URI>`), shorthand with `!!` default handle
(`!!str` → `tag:yaml.org,2002:str`), shorthand with user-defined
handles from `%TAG` directives, and local tags (`!suffix`). Resolved
tags are included in `Scalar`, `SequenceStart`, and `MappingStart`
events.
**Complexity:** Medium
**Comment:** Tag resolution against `%TAG` directives is performed
at scan time. The resolved string is `Cow::Borrowed` for verbatim
tags and `Cow::Owned` for expanded shorthands.
**Tier:** 1

### Block Scalar Chomping [completed]

**Description:** All three chomping modes are supported for block
scalars: strip (`-`), clip (default), and keep (`+`). The `Chomp`
enum is part of `ScalarStyle::Literal(Chomp)` and
`ScalarStyle::Folded(Chomp)`.
**Complexity:** Low
**Comment:** Chomping controls trailing newline handling. All three
modes are required for spec conformance.
**Tier:** 1

### Multi-document Support [completed]

**Description:** A YAML stream can contain multiple documents
separated by `---` and optionally terminated by `...`. The parser
emits `DocumentStart`/`DocumentEnd` events for each document, carries
the `%YAML` version and `%TAG` directives in `DocumentStart`, and the
loader returns a `Vec<Document<Span>>`.
**Complexity:** Low
**Comment:** Each document gets a fresh anchor map in the loader;
anchors do not cross document boundaries.
**Tier:** 1

### Loader Conformance — Full AST Fidelity [completed]

**Description:** The `load()` API passes 375/375 loader conformance
cases derived from the YAML Test Suite. Every valid input that the
event stream accepts is correctly materialized into a `Vec<Document>`,
preserving scalars, collections, anchors, tags, multi-document
streams, and empty documents.
**Complexity:** High
**Comment:** A correct event stream does not automatically imply a
correct AST — the loader is a separate conformance surface that must
be tested independently. Gaps found and fixed include empty-document
handling and anchor/alias resolution edge cases.
**Tier:** 1

### Document Marker Flags in AST [completed]

**Description:** `Document<Loc>` exposes two new boolean fields:
`explicit_start` (set when the document begins with a `---` marker)
and `explicit_end` (set when the document ends with a `...` marker).
The flags are populated by the loader from `DocumentStart`/
`DocumentEnd` events and preserved in the AST for downstream consumers
(e.g. the formatter round-trips these markers faithfully).
**Complexity:** Low
**Comment:** Required for formatter conformance — documents with
explicit `---`/`...` markers must have them preserved in formatted
output. The flags are sourced from the event stream, so no extra
parsing is needed; the loader already consumed both events.
**Tier:** 1

### YAML 1.2 Conformance [completed]

**Description:** The parser passes 368/368 cases in the YAML Test
Suite (all valid and invalid test cases). Spec-faithful implementation
following YAML 1.2 §§5–9.
**Complexity:** High
**Comment:** 100% conformance on the authoritative test suite.
Achieved across block sequences/mappings, flow collections, all scalar
styles, directives, anchors, aliases, multi-document streams, and
error cases.
**Tier:** 1

### All Scalar Styles [completed]

**Description:** All five YAML scalar styles are supported: plain,
single-quoted, double-quoted, literal block (`|`), and folded block
(`>`). Line folding is applied for folded scalars. Escape sequences
are decoded for double-quoted scalars.
**Complexity:** Medium
**Comment:** The scalar value in events is the fully decoded logical
content — callers do not need to handle quoting or escape sequences.
**Tier:** 1

### YAML Directives (`%YAML`, `%TAG`) [completed]

**Description:** `%YAML` version directives and `%TAG` handle
directives are parsed and scoped to the document they precede. The
version tuple is carried in `DocumentStart`. Custom tag prefixes from
`%TAG` are applied during tag resolution.
**Complexity:** Low
**Comment:** Directive scope resets at each `---` marker, matching
YAML 1.2 §6.8.
**Tier:** 2

### Explicit Keys (`? key:`) [completed]

**Description:** YAML explicit mapping keys (`? ` indicator) are
supported, including multi-line key content and keys that are
themselves block sequences or mappings.
**Complexity:** Medium
**Comment:** Explicit keys interact with `? ` on the preceding line
and block sequence indicators — handled via `explicit_key_pending`
state.
**Tier:** 2

### §10 Schema Resolution [completed]

**Description:** The loader applies YAML 1.2.2 §10 schema tag resolution to every `load()` call.
`Schema::Core` (§10.3) is the default, matching the YAML spec recommendation for processors.
Three schemas are selectable via `LoaderBuilder::schema(schema)`:
- `Schema::Failsafe` (§10.1) — all untagged scalars resolve to `tag:yaml.org,2002:str`; all
  untagged sequences to `tag:yaml.org,2002:seq`; all untagged mappings to `tag:yaml.org,2002:map`.
  The `!` non-specific tag resolves by kind.
- `Schema::Json` (§10.2) — untagged plain scalars are matched against the JSON pattern table
  (`null`, `true|false`, integer, float); non-matching plain scalars are rejected with
  `LoadError::UnresolvedScalar`. Non-plain scalars resolve to `str`. Untagged collections resolve
  by kind.
- `Schema::Core` (§10.3, default) — superset of JSON; unmatched plain scalars fall back to
  `tag:yaml.org,2002:str` instead of being rejected.

Explicit source tags always take precedence over schema-derived resolution. Schema-resolved tags
have `tag_loc: None` (no source position); source-tagged nodes have `tag_loc: Some`. Callers
that need raw unresolved tags should use `parse_events()`, which is schema-agnostic.
**Complexity:** Medium
**Comment:** `Schema::Core` as the default follows YAML 1.2.2 §10.3 ("The Core Schema is the
recommended default schema that YAML [processors] should use unless instructed otherwise").
Schema resolution is decoupled from the streaming event layer and lives entirely in the loader.
**Tier:** 1

### Event and Node Variant Memory Layout Optimization [completed]

**Description:** Two-stage restructuring of the hot types in the event pipeline:

*Stage A — Node variants.* `Node::Scalar`, `Node::Mapping`, and `Node::Sequence`
carry rare fields (`anchor`, `anchor_loc`, `tag_loc`, `leading_comments`,
`trailing_comment`) behind `meta: Option<Box<NodeMeta>>`. `Node<Span>` size:
288 bytes → 120 bytes per variant.

*Stage B — Event variants.* `Event::Scalar`, `Event::SequenceStart`, and
`Event::MappingStart` carry their anchor and tag fields (`anchor`, `anchor_loc`,
`tag`, `tag_loc`) behind `meta: Option<Box<EventMeta<'input>>>`. The common
case — no anchor, no source-text tag — pays only one 8-byte pointer; source-text
tags and anchors are rare in block-heavy and Kubernetes documents.
`Event` size: 40 bytes (was ~112 bytes per node variant with four inline fields).
Accessor methods `anchor()`, `anchor_loc()`, `tag()`, `tag_loc()` on `Event`
replace direct field access; patterns that previously destructured these four
fields by name must use the accessor methods.
**Complexity:** Medium
**Comment:** Stage A is a semver-breaking API change. Stage B extends
it without an additional version bump — the accessor-method migration is the
same pattern as Stage A. The `tag` field is boxed in `EventMeta` (unlike `Node`
where tag is kept inline because the schema resolver populates it on every loaded
node); events carry a tag only when the source text contained one, which is rare.
**Tier:** 2

### Zero-Allocation Resolver-Injected Tags [completed]

**Description:** `Node::tag` changed from `Option<String>` to
`Option<Cow<'static, str>>`. Tags injected by the schema resolver
(`apply_schema_to_node`) are now `Cow::Borrowed(&'static str)`,
eliminating four heap allocations per loaded node in typical documents.
User-authored tags from the input stream remain `Cow::Owned(String)`.
Callers that previously read `tag` as `Option<String>` must update to
`Option<Cow<'static, str>>` — `as_deref()` and string comparisons
continue to work unchanged via `Deref<Target = str>`.
**Complexity:** Low
**Comment:** The `'static` lifetime bound matches `ResolvedTag::as_str()`
which returns `&'static str` constants. User-authored tags need owned
storage because they are derived from the input buffer which does not
outlive the AST. This is a semver-breaking API change (0.6 → 0.7).
**Tier:** 2

### Block-Sequence Plain Scalar Fast Path [completed]

**Description:** A scan optimization for the common pattern of a
plain scalar on a block-sequence line (`- value`). The fast path
avoids re-scanning the plain scalar from the raw line buffer, reducing
per-item overhead in large flat sequences.
**Complexity:** Low
**Comment:** Added after profiling showed block-sequence parsing as
the hottest path for typical Kubernetes YAML. Verified by the
existing benchmark suite.
**Tier:** 2

---

### Schema-Based Type Coercion [won't implement]

**Description:** Interpret scalar values according to a schema
(e.g. coerce `"true"` → `bool`, `"42"` → `i64`) in the parser
itself.
**Complexity:** Medium
**Comment:** Type coercion is a loader/application concern, not a
parser concern. The parser's job is to produce the logical scalar
string — callers apply their own schema. Adding coercion would couple
the parser to schema logic and make it unsuitable as a general-purpose
streaming layer.
**Tier:** 4

### Pull-Based (Push) Incremental Parsing [won't implement]

**Description:** Accept input in chunks, allowing parsing of very
large YAML streams that do not fit in memory.
**Complexity:** Very High
**Comment:** The parser operates on a `&str` slice. Incremental
chunked input would require a fundamental redesign of the lexer to
handle tokens that span chunk boundaries. The LSP use case operates
on whole documents held in memory by the editor.
**Tier:** 4
