**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-29

## Goal

Reframe the root README.md and CONTRIBUTING.md to remove all
references to AI authorship. The new messaging focuses on what
the project delivers — lightweight, fast Rust language servers
with minimal memory footprint — rather than how it was built.

## Context

- The current README tagline is "built entirely by AI agents"
  with "No human-written application code" — this framing can
  be off-putting and distracts from the project's value
- The contributing model (issues only, no PRs) stays the same,
  but the justification changes from "AI-written" to maintainer
  preference
- CONTRIBUTING.md opens with "This project is AI-written" — that
  paragraph needs rewriting
- The rlsp-yaml crate README has no AI references — no changes needed
- CLAUDE.md (internal project docs) keeps its AI references since
  it's not user-facing

### Files involved

- `README.md` — root project README
- `CONTRIBUTING.md` — contribution guidelines

## Steps

- [x] Clarify requirements with user
- [ ] Update README.md — new tagline, remove AI references, reframe contributing section
- [ ] Update CONTRIBUTING.md — remove AI opening paragraph, reframe policy

## Tasks

### Task 1: Reframe README.md and CONTRIBUTING.md

Update both files in a single commit since they're closely related
and the changes are small.

**README.md changes:**
- Replace the tagline "built entirely by AI agents. No human-written
  application code — every line of source was authored, reviewed, and
  committed by AI." with messaging focused on lightweight, fast
  implementations with minimal memory footprint
- Update the Contributing section — keep issues-only policy, remove
  "all implementation is done by AI", frame as maintainer preference

**CONTRIBUTING.md changes:**
- Replace the opening paragraph ("This project is AI-written...")
  with a neutral project description
- Keep all other content (how to contribute, bug reports, feature
  requests, labels) unchanged

## Decisions

- **New angle:** Lightweight & fast — this is the project's actual
  differentiator (stated in CLAUDE.md's overview as the purpose)
- **Contributing policy unchanged:** Issues-only stays, just drop
  the AI justification
- **CLAUDE.md untouched:** Internal agent docs keep AI references
  since they're not user-facing and agents need that context
