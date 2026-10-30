# rlsp-yaml-parser

Spec-faithful streaming YAML 1.2.2 parser. Exposes two independently
conformance-tested APIs: a flat event stream (`parse_events()`) and an AST
loader (`load()`). See `README.md` for usage and `docs/conformance/README.md`
for full conformance status.

## Conventions

<!-- Agents: add non-obvious project conventions discovered during work — things a future agent would need to know to avoid mistakes. One line each. Remove when no longer true. -->

- Public `pub struct` and `pub enum` types must be `#[non_exhaustive]` — internal evolution stays semver-compatible for external consumers; mark individual variants `#[non_exhaustive]` when they gain new fields.

## Spec and Reference Parser

Every change to parser behavior must conform to the YAML 1.2.2 spec. A green test suite does not show that: the yaml-test-suite misses many shapes, so a change can pass it and still reject valid YAML or accept invalid YAML.

1. **Read the spec.** Look up the production or prose that decides the changed behavior in the [spec text](https://yaml.org/spec/1.2.2/) and name it in the handoff. Memory is not a check.
2. **Compare with libfyaml.** Compare `parse_events` with `fy-tool --testsuite <file>` from the latest stable [libfyaml release](https://github.com/pantoniou/libfyaml/releases) on the changed inputs and the yaml-test-suite inputs, and compare with the last `rlsp-yaml-parser-v*` tag so that everything since the release is covered. The distro package in the devcontainer is older and has no `fy-tool`; build the release tarball outside the repo.
3. **Settle every deviation.** A difference is either fixed here or reported to the user with the spec production, and the user decides; no agent accepts one alone. libfyaml can be the wrong side: read the spec, check its [issue tracker](https://github.com/pantoniou/libfyaml/issues), then decide. A deviation that stays goes into `docs/conformance/design-decisions.md`.

## Conformance Sync

<!-- Agents: update this table when new enforcement sites or conformance consumers are added. When fixing a conformance finding, update the corresponding docs/conformance/ entry and add a /// doc comment at the enforcement site in the same commit. -->

The parser enforces YAML 1.2.2 BNF productions and normative prose. When
adding or changing enforcement, keep the following locations in sync:

| Source of truth | Consumers | Sync when |
|-----------------|-----------|-----------|
| Enforcement site in source (e.g., `chars.rs`, `directive_scope.rs`) | `docs/conformance/bnf-§N.md` entry for the affected production | Any change to character-set predicates, tag validation, schema matchers, or indent enforcement |
| `docs/conformance/design-decisions.md` | Feature log (`docs/feature-log.md`) user-facing entries | Stricter-than-spec or formally-accepted-lenient decision added or changed |
| `docs/conformance/README.md` pass-rate table | README.md conformance section | Conformance pass rate changes |
