# Feature Log

Feature decisions for rlsp-fmt, newest first. Tiered by
user impact, implementation feasibility, and alignment
with existing infrastructure.

**Tiers:**
- **1** — High impact, feasible now
- **2** — Medium impact, moderate effort
- **3** — Valuable but higher effort
- **4** — Niche or high effort / low return

---

### Wadler-Lindig Pretty-Printing Algorithm [completed]

**Description:** Core algorithm from Lindig's "Strictly Pretty"
paper. Builds a `Doc` IR tree that describes document structure
independently of rendering. The printer resolves `Group` nodes
by attempting flat mode (everything on one line) and falling
back to break mode (line breaks expanded) when content would
exceed `print_width`. Uses an explicit work stack — no recursion
— so deeply nested documents do not overflow the call stack.
**Complexity:** Medium
**Comment:** The algorithm is a faithful implementation of the
Lindig variant (iterative, not Wadler's original continuation
style). The explicit stack is a deliberate choice over recursion
for safety and performance.
**Tier:** 1

### Group Optimization with Flat-Mode Lookahead [completed]

**Description:** `group(doc)` marks a subtree as a flat/break
decision boundary. The printer runs a `fits` lookahead before
committing to flat mode: if the group's content would exceed the
remaining line width, the printer switches to break mode. The
lookahead is also short-circuited by `HardLine` — any mandatory
line break in a group forces break mode regardless of width.
**Complexity:** Medium
**Comment:** The `fits` lookahead uses its own flat-mode stack and
tracks remaining width via saturating subtraction. `HardLine`
short-circuits immediately — this avoids scanning large groups
when a forced break is inevitable.
**Tier:** 1

### `FlatAlt` — Mode-Dependent Content [completed]

**Description:** `flat_alt(flat_doc, break_doc)` renders different
content depending on whether the enclosing group is in flat or
break mode. Used to produce mode-sensitive alternatives such as a
space in flat mode and a newline-with-content in break mode.
**Complexity:** Low
**Comment:** Enables constructs like Prettier's `ifBreak` — content
that only appears when the group breaks. Directly models the
`FlatAlt` constructor from the Lindig paper.
**Tier:** 1

### Configurable Indentation [completed]

**Description:** `FormatOptions` exposes two indentation controls:
`tab_width` (number of spaces per indent level, default 2) and
`use_tabs` (emit tab characters instead of spaces, default false).
When `use_tabs` is true, the column-width calculation for `fits`
counts each tab as one column to maintain conservative fit
estimates.
**Complexity:** Low
**Comment:** Tabs-vs-spaces is a project-level style choice. Counting
tabs as 1 column for fit checks is a conservative approximation —
real tab width depends on the viewer's tab-stop setting, which the
printer cannot know.
**Tier:** 1

### Configurable Print Width [completed]

**Description:** `FormatOptions::print_width` sets the maximum
line width before groups break (default 80). The printer measures
column position after each `Text` and `Line` emission and uses the
remaining width in the `fits` lookahead.
**Complexity:** Low
**Comment:** Print width is the primary knob for controlling output
density. Default 80 follows common convention (Prettier, rustfmt).
**Tier:** 1

### `join` Combinator [completed]

**Description:** `join(separator, docs)` intersperses a separator
document between each element of a list. Returns an empty `Concat`
for an empty list. Used to build comma-separated lists, path
components, and other delimited sequences without manual iteration.
**Complexity:** Low
**Comment:** A convenience combinator that eliminates the common
pattern of manually tracking whether a separator is needed before
each item.
**Tier:** 1

### Hard Lines [completed]

**Description:** `hard_line()` produces a mandatory line break that
bypasses flat-mode lookahead. Always emits a newline followed by
the current indentation. Forces the enclosing group into break mode
when encountered in the `fits` lookahead.
**Complexity:** Low
**Comment:** Required for content that must always appear on its own
line regardless of available width (e.g. YAML block scalar content,
comment lines).
**Tier:** 1

### Soft Lines [completed]

**Description:** `line()` produces a context-sensitive break: a
single space in flat mode and a newline followed by the current
indentation in break mode. The most commonly used line-break node
for optional wrapping.
**Complexity:** Low
**Comment:** Soft lines are the mechanism that makes groups
width-sensitive. In flat mode the entire group flows on one line
with spaces; in break mode each soft line becomes an indented
newline.
**Tier:** 1

### Nested Indentation [completed]

**Description:** `indent(doc)` increases the indentation level for
its child by one step. Indentation levels stack — nested `indent`
calls accumulate. The actual indentation string is computed from
the level at line-break time using `tab_width` or tab characters.
**Complexity:** Low
**Comment:** Indentation is relative, not absolute — each `indent`
wrapping adds one level relative to the current context. This means
a document can be embedded at any nesting depth without rewriting
indentation values.
**Tier:** 1

---

### Alignment [won't implement]

**Description:** Align subsequent lines to a column position
rather than an indentation level — e.g. align all values in a
record at column N regardless of nesting.
**Complexity:** High
**Comment:** Alignment requires tracking absolute column positions
through the work stack in addition to relative indentation levels,
complicating the state machine significantly. The YAML formatter
(the primary consumer) does not need alignment — YAML indentation
is always relative.
**Tier:** 4

### Line Suffix [won't implement]

**Description:** Content that is appended to the end of the
current line after all other content — used in some formatters
for trailing comments.
**Complexity:** Medium
**Comment:** Line suffixes require a two-pass or deferred-emission
strategy because the suffix must appear after content that hasn't
been emitted yet. The YAML formatter attaches trailing comments
via the loader's `trailing_comment` field instead.
**Tier:** 4
