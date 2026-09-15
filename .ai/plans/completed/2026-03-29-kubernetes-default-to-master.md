**Repository:** root
**Status:** Completed (2026-03-29)
**Created:** 2026-03-29

## Goal

Change the default Kubernetes schema version from a
hardcoded `"1.32.0"` to `"master"`, so the language server
always resolves schemas against the latest Kubernetes
definitions without requiring code updates for each K8s
release. Users can still override with a specific version
via the `kubernetesVersion` setting.

## Context

- Schema source: `yannh/kubernetes-json-schema` on GitHub
- Versioned directories use `v{version}-standalone-strict/`
  but `master` uses `master-standalone-strict/` (no `v`
  prefix)
- This matches Kubeconform's default behavior
- The `kubernetesVersion` setting still allows pinning to a
  specific version (e.g. `"1.32.0"`)
- Key files: `rlsp-yaml/src/schema.rs` (URL construction),
  `rlsp-yaml/src/server.rs` (default constant + settings),
  `rlsp-yaml/docs/configuration.md` (user docs),
  `rlsp-yaml/tests/lsp_lifecycle.rs` (integration tests)

## Steps

- [x] Clarify requirements with user
- [x] Update URL construction to handle "master" vs versioned
- [x] Change default constant to "master"
- [x] Update documentation
- [x] Add/update tests

## Tasks

### Task 1: Update URL construction and default, with tests

*Completed — commit c0b7367*

Change `kubernetes_schema_url()` in `schema.rs` to produce
`master-standalone-strict/` when `k8s_version` is `"master"`
and `v{version}-standalone-strict/` otherwise. Change
`DEFAULT_KUBERNETES_VERSION` in `server.rs` from `"1.32.0"`
to `"master"`. Update the doc comment on the
`kubernetes_version` field. Add a unit test for the
`"master"` URL case in `schema.rs`.

Files:
- `rlsp-yaml/src/schema.rs` — conditional in URL format
- `rlsp-yaml/src/server.rs` — default constant + doc comment

### Task 2: Update documentation

*Completed — included in commit c0b7367 (reviewer caught
stale references during Task 1 review)*

Update `rlsp-yaml/docs/configuration.md` to reflect the new
default of `"master"`. Update the description to explain
that `"master"` tracks the latest Kubernetes schemas and
that users can pin a specific version.

Files:
- `rlsp-yaml/docs/configuration.md`

## Decisions

- **Use `master-standalone-strict/`** — matches Kubeconform's
  default behavior; always up-to-date without code changes.
  Trade-off: may include unreleased K8s API changes, but
  users can pin a specific version if needed.
- **Conditional URL construction** — `"master"` is the only
  special case (no `v` prefix); all other version strings
  get `v{version}` prefix as before.
