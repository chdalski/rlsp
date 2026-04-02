# Project Context — Output Format

Generated `CLAUDE.md` files provide context that agents
cannot infer from code alone: what the project is, how to
verify work, and what non-obvious rules apply. Agents
discover file structure, languages, and dependencies on
their own — CLAUDE.md should not duplicate that.

Target under 80 lines for most projects; under 200 for
complex monorepos with many components.

## Sections

Every generated `CLAUDE.md` starts with a level-1 heading
(the project name) followed by a 2-4 sentence overview
synthesized from README.md and manifest `description`
fields — what the project is, who it serves, why it
exists. If no README exists, infer from code structure
and manifest metadata.

**Build and Test** — always present. A level-2 section
with a shell code block listing build, test, lint, format,
and clean commands detected from manifests and config
files. Group by language or component when the project
uses multiple. These are the commands agents need to
verify their work — without them, agents guess or search.

**Components** — present only for workspaces and monorepos.
A level-2 section with a markdown table (columns: Path,
Purpose). One row per workspace member or sub-project.
Synthesize purpose from each component's README.md first
paragraph or manifest `description` field. Omit this
section entirely for single-project repos.

**Conventions** — always present. A level-2 section
containing non-obvious project conventions detected during
scanning and confirmed by the user. Each entry is one
line. Preceded by an HTML comment that enables progressive
enrichment — agents add entries during normal work when
they discover conventions not yet documented:

`<!-- Agents: add non-obvious project conventions
discovered during work — things a future agent would need
to know to avoid mistakes. One line each. Remove when no
longer true. -->`

Write the section header and HTML comment even when no
conventions were detected — the empty section signals to
agents that they can add entries.

**References** — always present. Same pattern as
Conventions but for authoritative URLs: specifications,
API docs, RFCs, design docs. The HTML comment is:

`<!-- Agents: add authoritative sources used to make
implementation decisions. One line each. -->`

Always include the header and comment, even when empty.

## Re-generation

When `/project-init` runs on an existing `CLAUDE.md`:

- **Regenerate**: Overview, Build and Test, Components —
  these reflect current project state and should be
  refreshed from manifests and README
- **Preserve**: Conventions and References entries — these
  contain user-confirmed and agent-discovered content that
  cannot be re-detected automatically
- **Add**: newly detected conventions or references not
  already present in the preserved entries
