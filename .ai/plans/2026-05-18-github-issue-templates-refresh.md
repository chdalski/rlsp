**Repository:** root
**Status:** InProgress
**Created:** 2026-05-18

# Refresh GitHub Issue Templates for Multi-Component Repo

## Goal

The `.github/ISSUE_TEMPLATE/` directory contains a single
generic bug template (`bug_report.yml`) and a single
feature template (`feature_request.yml`), both framed as
rlsp-yaml-only. The repository now ships five distinct
user-facing components — the `rlsp-yaml` LSP, the
`rlsp-yaml-parser` parser library, the `rlsp-fmt`
pretty-printer engine, the VS Code extension (CalVer
versioned), and the Zed extension (semver versioned) — and
reporters cannot indicate which component a bug or feature
applies to. Replace the single bug template with five
per-component bug templates that request the right
reproduction details for each component, replace the
feature template with a single form that has a required
component dropdown, and update `CONTRIBUTING.md` so its
"Bug Reports" wording and its "Recommended GitHub Labels"
table match the new layout.

## Context

Components and version schemes (sources: root `README.md`,
root `CLAUDE.md`, `Cargo.toml` workspace members):

- `rlsp-yaml` — YAML language server, semver (e.g.
  `0.3.1`).
- `rlsp-yaml-parser` — spec-faithful streaming YAML 1.2
  parser library, semver. Exposes two APIs that may each
  produce bugs independently: `parse_events()` and
  `load()`.
- `rlsp-fmt` — generic Wadler-Lindig pretty-printer
  engine, semver. YAML-agnostic; bugs reproduce from a
  Doc IR snippet or a short Rust caller.
- VS Code extension — CalVer, `YYYY.MM.NN` (e.g.
  `2026.05.01`). Built per-platform (Linux/macOS/Windows
  × x64/arm64) per `README.md`. Tag format
  `vscode-v<YYYY.MM.NN>`.
- Zed extension — semver. Tag format `zed-v<semver>`.

Existing files in `.github/`:

- `.github/ISSUE_TEMPLATE/bug_report.yml` — single bug
  template, requires "rlsp-yaml Version" and editor/OS.
- `.github/ISSUE_TEMPLATE/feature_request.yml` — single
  feature template, "Suggest a feature for rlsp-yaml".
- `.github/ISSUE_TEMPLATE/config.yml` — disables blank
  issues, links to Discussions. Stays unchanged.
- `.github/PULL_REQUEST_TEMPLATE.md` — states external
  PRs are not accepted. Stays unchanged.

`CONTRIBUTING.md` cross-references the templates:

- "Bug Reports" section says "Open an issue using the bug
  report template" (singular) and lists "Version of
  `rlsp-yaml` and your editor/LSP client" as required —
  both must change to fit a multi-component layout.
- The "Recommended GitHub Labels" table lists `bug`,
  `enhancement`, `question`, `accepted`, `wontfix`,
  `duplicate` but does not list any component-routing
  labels. The new templates apply component labels for
  triage, so the table must list them.

User decisions captured during clarification:

- Five per-component bug templates, not one template with
  a dropdown.
- One unified feature template with a required component
  dropdown — features are usage-driven and rarely need
  component-specific fields.
- Editor/OS fields stay on bug templates where they
  matter (LSP, VS Code extension, Zed extension) and are
  omitted from parser/fmt bug templates where they do not
  apply.

Specifications and reference:

- GitHub issue forms schema:
  https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-githubs-form-schema
- GitHub issue forms must be valid YAML; GitHub renders
  the form from this file when a reporter selects the
  template.

Label scheme the new templates apply (added to the
recommended-labels table):

| Label | Color | Description |
|-------|-------|-------------|
| `rlsp-yaml` | `#fbca04` | Affects the YAML language server |
| `rlsp-yaml-parser` | `#fef2c0` | Affects the YAML 1.2 parser library |
| `rlsp-fmt` | `#c5def5` | Affects the pretty-printing engine |
| `vscode-extension` | `#0052cc` | Affects the VS Code extension |
| `zed-extension` | `#5319e7` | Affects the Zed extension |

These colors are visually distinct from each other and
from the existing labels (`#d73a4a` red, `#a2eeef` light
blue, `#d876e3` pink, `#0e8a16` green, `#ffffff` white,
`#cfd3d7` grey).

## Steps

- [x] Add five per-component bug templates and a unified
      feature template, remove the obsolete generic bug
      template, and update `CONTRIBUTING.md` to match.

## Tasks

### Task 1: Restructure issue templates and align CONTRIBUTING.md

**Commit:** `5fd7155`

Replace `bug_report.yml` with five per-component bug
templates, replace `feature_request.yml` with a unified
form that has a required component dropdown, and update
`CONTRIBUTING.md`'s "Bug Reports" wording and
"Recommended GitHub Labels" table to match. The templates
form one coherent triage system; landing them piecewise
would show users an inconsistent chooser between commits.
Sub-tasks track per-template progress; the whole set
commits together.

Files created:

- `.github/ISSUE_TEMPLATE/bug_rlsp_yaml.yml`
- `.github/ISSUE_TEMPLATE/bug_rlsp_yaml_parser.yml`
- `.github/ISSUE_TEMPLATE/bug_rlsp_fmt.yml`
- `.github/ISSUE_TEMPLATE/bug_vscode_extension.yml`
- `.github/ISSUE_TEMPLATE/bug_zed_extension.yml`

Files modified:

- `.github/ISSUE_TEMPLATE/feature_request.yml` (replaced
  in place)
- `CONTRIBUTING.md`

Files deleted:

- `.github/ISSUE_TEMPLATE/bug_report.yml`

Sub-tasks (each lists the exact field shape required):

- [x] Create `.github/ISSUE_TEMPLATE/bug_rlsp_yaml.yml`.
  Metadata: `name: "Bug Report: rlsp-yaml (language
  server)"`, `description: "Report a bug in the rlsp-yaml
  language server"`, `labels: ["bug", "rlsp-yaml"]`.
  Required fields (textarea unless noted): Description,
  Steps to Reproduce, Expected Behavior, Actual Behavior,
  rlsp-yaml Version (input, placeholder `e.g. 0.3.1`).
  Optional fields: Editor / LSP Client (input, placeholder
  `e.g. Neovim 0.10, VS Code 1.88`), Operating System
  (input, placeholder `e.g. Ubuntu 24.04, macOS 14,
  Windows 11`), YAML Sample (textarea, `render: yaml`).
- [x] Create
  `.github/ISSUE_TEMPLATE/bug_rlsp_yaml_parser.yml`.
  Metadata: `name: "Bug Report: rlsp-yaml-parser"`,
  `description: "Report a bug in the YAML 1.2 parser
  library"`, `labels: ["bug", "rlsp-yaml-parser"]`.
  Required fields: Description (textarea), YAML Sample
  (textarea, `render: yaml`), Expected Parse Result
  (textarea — events from `parse_events()` or AST from
  `load()`), Actual Parse Result (textarea),
  rlsp-yaml-parser Version (input, placeholder
  `e.g. 0.3.1`). Optional: API used (dropdown,
  options: `parse_events()`, `load()`, Unsure), YAML
  Test Suite reference (input, placeholder `e.g. 6XDY`).
  Editor/OS fields are omitted — the parser is a library
  and does not depend on either.
- [x] Create `.github/ISSUE_TEMPLATE/bug_rlsp_fmt.yml`.
  Metadata: `name: "Bug Report: rlsp-fmt"`,
  `description: "Report a bug in the pretty-printing
  engine"`, `labels: ["bug", "rlsp-fmt"]`. Required
  fields: Description (textarea), Reproduction (textarea,
  `render: rust` — a short Doc IR snippet or caller),
  Expected Output (textarea), Actual Output (textarea),
  rlsp-fmt Version (input, placeholder `e.g. 0.3.1`).
  Editor/OS fields are omitted — the engine is
  language-agnostic and does not run inside an editor on
  its own.
- [x] Create
  `.github/ISSUE_TEMPLATE/bug_vscode_extension.yml`.
  Metadata: `name: "Bug Report: VS Code Extension"`,
  `description: "Report a bug in the rlsp-yaml VS Code
  extension"`, `labels: ["bug", "vscode-extension"]`.
  Required fields: Description (textarea), Steps to
  Reproduce (textarea), Expected Behavior (textarea),
  Actual Behavior (textarea), Extension Version (input,
  placeholder `e.g. 2026.05.01` — flag CalVer in the
  field description). Optional fields: VS Code Version
  (input, placeholder `e.g. 1.88.0`), Operating System
  (input, placeholder `e.g. Ubuntu 24.04, macOS 14,
  Windows 11`), Architecture (dropdown, options: `x64`,
  `arm64`, `Unsure`), YAML Sample (textarea,
  `render: yaml`), OUTPUT panel log (textarea).
- [x] Create
  `.github/ISSUE_TEMPLATE/bug_zed_extension.yml`.
  Metadata: `name: "Bug Report: Zed Extension"`,
  `description: "Report a bug in the rlsp-yaml Zed
  extension"`, `labels: ["bug", "zed-extension"]`.
  Required fields: Description (textarea), Steps to
  Reproduce (textarea), Expected Behavior (textarea),
  Actual Behavior (textarea), Extension Version (input,
  placeholder `e.g. 0.1.1`). Optional fields: Zed Version
  (input, placeholder `e.g. 0.150.0`), Operating System
  (input, placeholder `e.g. Ubuntu 24.04, macOS 14,
  Windows 11`), YAML Sample (textarea, `render: yaml`),
  Zed log excerpt (textarea).
- [x] Replace `.github/ISSUE_TEMPLATE/feature_request.yml`
  with a unified form. Metadata: `name: "Feature
  Request"`, `description: "Suggest a feature for one of
  the RLSP crates or editor extensions"`,
  `labels: ["enhancement"]`. First field: Component
  (dropdown, required, options exactly: `rlsp-yaml
  (YAML language server)`, `rlsp-yaml-parser (parser
  library)`, `rlsp-fmt (pretty-printer engine)`, `VS
  Code extension`, `Zed extension`, `Unsure / multiple`).
  Required fields: Use Case (textarea), Proposed
  Behavior (textarea). Optional: Alternatives Considered
  (textarea), Example (textarea — generic so the field
  fits non-YAML components; no `render` directive).
- [x] Delete `.github/ISSUE_TEMPLATE/bug_report.yml`. The
  five per-component bug templates replace it; leaving
  the generic file alongside would show users a duplicate
  generic entry in the chooser.
- [x] Update `CONTRIBUTING.md` "Bug Reports" section:
  replace the sentence "Open an issue using the bug
  report template." with "Open the bug report template
  for the affected component — `rlsp-yaml`,
  `rlsp-yaml-parser`, `rlsp-fmt`, the VS Code extension,
  or the Zed extension." Replace the bullet "Version of
  `rlsp-yaml` and your editor/LSP client" with two
  bullets: "Version of the affected component (required)"
  and "Editor and OS — optional, but helpful for
  `rlsp-yaml` and editor extension bugs".
- [x] Update `CONTRIBUTING.md` "Recommended GitHub Labels"
  table: append five rows with the colors listed in this
  plan's Context section — `rlsp-yaml` (`#fbca04`),
  `rlsp-yaml-parser` (`#fef2c0`), `rlsp-fmt` (`#c5def5`),
  `vscode-extension` (`#0052cc`), `zed-extension`
  (`#5319e7`). Each row's description matches the table
  in the Context section.
- [x] Verify every new and modified `.yml` file in
  `.github/ISSUE_TEMPLATE/` parses as valid YAML. Use any
  YAML parser available in the environment (`node -e
  "require('js-yaml').load(require('fs').readFileSync('PATH','utf8'))"`
  is one option). Run the verifier against all six
  templates (`bug_rlsp_yaml.yml`,
  `bug_rlsp_yaml_parser.yml`, `bug_rlsp_fmt.yml`,
  `bug_vscode_extension.yml`, `bug_zed_extension.yml`,
  `feature_request.yml`). Record the verifier command and
  its zero-exit output in the handoff.
- [x] Verify `.github/ISSUE_TEMPLATE/` after the change
  contains exactly these seven files and no others:
  `bug_rlsp_yaml.yml`, `bug_rlsp_yaml_parser.yml`,
  `bug_rlsp_fmt.yml`, `bug_vscode_extension.yml`,
  `bug_zed_extension.yml`, `feature_request.yml`,
  `config.yml`. Include an `ls .github/ISSUE_TEMPLATE/`
  in the handoff.

## Decisions

- **One task, sub-tasks per file** — the templates form a
  single coherent triage system; the chooser would be
  inconsistent if templates landed in separate commits
  (e.g., new component templates next to the old generic
  one). The whole set commits together.
- **Per-component bug templates, unified feature
  template** — user-directed during clarification. Bugs
  need component-specific reproduction fields (LSP wants
  editor info, parser wants a YAML sample, fmt wants Doc
  IR); features are usage-driven and the shape is the
  same everywhere.
- **Editor/OS fields optional on every template where
  they appear; omitted from parser and fmt templates** —
  user-directed during clarification. Parser and fmt
  components run as libraries, so editor and OS fields
  are omitted from those two templates entirely. On the
  LSP and extension templates the fields stay (they
  matter when triaging editor-specific bugs) but are
  marked optional so reporters who hit a reproducer
  without that context can still file. The component's
  own version field stays required on every bug template
  — it identifies the affected build.
- **Component label routing** — the templates apply
  `bug` plus a component label so triagers can filter
  the queue by component without opening each issue.
  `CONTRIBUTING.md`'s "Recommended GitHub Labels" table
  is updated to document the new labels — without that,
  the templates produce labels the table does not list.
- **Filenames use underscores** — existing templates
  (`bug_report.yml`, `feature_request.yml`) use
  underscores; the new files follow the same convention.
- **`config.yml` unchanged** — blank issues stay disabled
  and the Discussions link remains valid.
- **`PULL_REQUEST_TEMPLATE.md` unchanged** — already
  correctly states external PRs are not accepted; nothing
  in it refers to issue-template structure.
- **No `docs/feature-log.md` entry in rlsp-yaml** — issue
  templates are repository infrastructure, not a
  user-facing rlsp-yaml feature; per project memory,
  feature-log is user-facing only.

## Non-Goals

- Adding a parser-conformance-failure template (could be
  added later if the volume of conformance reports
  warrants it; not requested in this clarification).
- Adding a documentation-issue template.
- Recoloring existing labels — only adding rows for new
  component-routing labels.
- Modifying `.github/dependabot.yml`,
  `.github/workflows/`, or any release-plz / CalVer
  tagging configuration.
- Creating the new labels in GitHub itself — the
  `CONTRIBUTING.md` table is a recommendation; actual
  label creation happens in the GitHub repo settings,
  which is the maintainer's action, not a code change.
