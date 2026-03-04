# Project Context

Project-specific context that all agents need regardless of
which files they touch. Auto-generated sections are filled
by `/project-init`; TODO sections need human curation.
This file is written to the project root as `CLAUDE.md`,
where Claude Code loads it at cache level 3 alongside
`.claude/CLAUDE.md` — both files are always in context.

## Overview

<!-- TODO: Add project name and a brief description of what
this project does, who it serves, and its core purpose.
Auto-detection cannot infer intent — only humans know why
the project exists. -->

## Languages and Frameworks

<!-- Auto-filled by /project-init — detected from manifest
files (package.json, Cargo.toml, pyproject.toml, go.mod).
Lists languages, runtime versions, and key framework
dependencies. -->

## Project Structure

<!-- Auto-filled by /project-init — key directories and
their purposes, detected from filesystem scanning. -->

## Active Rules

<!-- Auto-filled by /project-init — maps detected languages
to the blueprint's conditional rule files. Informational:
rules auto-load via paths: frontmatter, but this section
tells you what guidance is available and helps you decide
what to customize. -->

## Build and Test

<!-- Auto-filled by /project-init — build tools, test
frameworks, and key commands detected from config files
and manifests. -->

## Architecture

<!-- TODO: Describe the high-level architecture — layers,
modules, data flow, key abstractions. Auto-detection can
find files but cannot infer design intent or system
boundaries. -->

## Code Exemplars

<!-- TODO: List 2-3 files that best represent the project's
coding style and conventions. These serve as concrete
examples for agents to follow — style guides describe
principles, exemplars show them in practice. -->

## Anti-Patterns

<!-- TODO: List project-specific "never do this" patterns.
Every project accumulates hard-won knowledge about what
NOT to do — patterns that cause bugs, performance issues,
or maintenance pain in THIS specific codebase. -->

## Trusted Sources

<!-- TODO: List authoritative references for this project —
API docs, RFCs, internal design docs, team wikis. Agents
use these as ground truth when general knowledge conflicts
with project-specific conventions. -->
