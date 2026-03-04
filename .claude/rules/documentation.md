---
paths:
  - "**/README*"
  - "**/docs/**/*.md"
---

# Documentation Principles

These principles activate when writing or editing
documentation files (READMEs, docs/ markdown). They ensure
documentation stays accurate, useful, and maintainable.

## ARID — Accept Repetition In Documentation

Documentation will repeat things found in code. This is
acceptable and often necessary — readers should not need to
read source code to understand documentation. Minimize
redundancy where practical, but don't apply DRY dogmatically.

## Audience Awareness

Identify who you are writing for before you start:

- **Users** want results — how to install, configure, and
  use
- **Developers** want to contribute — how the code works,
  how to extend it, how to run tests

Tailor depth, vocabulary, and structure to the audience.
When a document serves both, separate the concerns into
distinct sections.

## Skimmability

Readers skim before they read. Structure content so they
can quickly find what they need:

- Use descriptive headings that summarize the section
  content
- Place the key idea first in each paragraph and list item
- Use lists and tables for structured information
- Keep paragraphs short and focused on one point

## Exemplary

Show, don't just tell — examples are the fastest path to
understanding:

- Include examples for common use cases
- Place examples near the concepts they illustrate
- Keep examples minimal — show the essential parts, omit
  boilerplate
- Separate examples from dense reference material so
  neither disrupts the other

## Consistency

Maintain uniform language and formatting throughout:

- Use the same term for the same concept everywhere — don't
  alternate between synonyms
- Follow the project's established formatting conventions
- When a project has a style guide, follow it strictly

## Currency

Wrong documentation is worse than missing documentation:

- Update docs whenever the code they describe changes
- Remove documentation for features that no longer exist
- Use version-agnostic language where possible to reduce
  maintenance burden
- When documentation must reference a specific version,
  make the version explicit

## Proximity

Store documentation close to the code it describes — this
makes it more likely to be updated when the code changes:

- Co-locate docs in the repository, not in external wikis
  or separate systems
- Place module-level docs near the module
- Inline documentation belongs where the behavior is
  non-obvious, not everywhere

## Completeness

Cover a topic fully or omit it entirely — partial coverage
without disclaimers misleads readers into thinking they
have the full picture:

- If a section is intentionally incomplete, say so
  explicitly
- A focused document that covers its scope thoroughly is
  better than a broad document that covers everything
  superficially

## Cumulative Structure

Order content so prerequisites come first — don't reference
concepts before introducing them:

- Build understanding progressively from simple to complex
- In tutorials, each step should build on the previous one
- In reference docs, organize by domain concept rather than
  implementation structure

## Anti-Patterns

### FAQ as Documentation

FAQs tend to become disorganized junk drawers. They
accumulate content that belongs in proper sections and go
stale quickly. Integrate answers into the relevant
documentation sections instead.

### Documenting Implementation Instead of Behavior

Describe what the code does for its users, not how it works
internally. Implementation details change frequently and
create maintenance burden. Document behavior, intent, and
contracts.

Exception: architecture documentation intentionally
describes implementation for developer audiences.

### Orphaned Documentation

Documentation that nobody maintains drifts from reality
and actively misleads. Every document should have a clear
relationship to code or a process that keeps it current.
If a document cannot be maintained, delete it.
