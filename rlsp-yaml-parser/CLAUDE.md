# rlsp-yaml-parser

Spec-faithful streaming YAML 1.2.2 parser. Exposes two independently
conformance-tested APIs: a flat event stream (`parse_events()`) and an AST
loader (`load()`). See `README.md` for usage and `docs/conformance/README.md`
for full conformance status.

## Conformance Sync

<!-- Agents: update this table when new enforcement sites or conformance consumers are added. When fixing a conformance finding, update the corresponding docs/conformance/ entry and add a /// doc comment at the enforcement site in the same commit. -->

The parser enforces YAML 1.2.2 BNF productions and normative prose. When
adding or changing enforcement, keep the following locations in sync:

| Source of truth | Consumers | Sync when |
|-----------------|-----------|-----------|
| Enforcement site in source (e.g., `chars.rs`, `directive_scope.rs`) | `docs/conformance/bnf-§N.md` entry for the affected production | Any change to character-set predicates, tag validation, schema matchers, or indent enforcement |
| `docs/conformance/design-decisions.md` | Feature log (`docs/feature-log.md`) user-facing entries | Stricter-than-spec or formally-accepted-lenient decision added or changed |
| `docs/conformance/README.md` pass-rate table | README.md conformance section | Conformance pass rate changes |
