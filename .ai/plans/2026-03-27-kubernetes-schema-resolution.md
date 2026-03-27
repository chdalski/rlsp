**Repository:** root
**Status:** InProgress
**Created:** 2026-03-27

## Goal

Automatically resolve JSON Schemas for Kubernetes manifests
by inspecting the document's root-level `apiVersion` and
`kind` fields. This eliminates the need for users to
manually configure schema associations for standard
Kubernetes resources — the server detects the resource type
and fetches the correct schema from
yannh/kubernetes-json-schema on GitHub.

## Context

- Current schema resolution: modeline → workspace glob →
  no schema. This feature adds a third fallback:
  modeline → workspace glob → Kubernetes auto-detect.
- Schema source: `https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v{version}-standalone-strict/{resource}.json`
- Naming convention verified:
  - Core API (`apiVersion: v1`, `kind: Pod`) →
    `pod-v1.json`
  - Grouped API (`apiVersion: apps/v1`,
    `kind: Deployment`) → `deployment-apps-v1.json`
  - All lowercase in the filename.
- New setting: `kubernetesVersion` (string, default
  `"1.32.0"`) — configurable so users can match their
  cluster version.
- Latest available version in the repo: `1.35.3`.
- The existing `SchemaCache`, `fetch_schema`, and
  `process_schema` infrastructure can be reused — the
  only new logic is detecting Kubernetes documents and
  constructing the schema URL.
- Key files: `server.rs` (integration), `schema.rs`
  (URL construction + detection), `configuration.md`,
  `feature-log.md`.
- Priority: modeline > workspace glob > K8s auto-detect.
  User-provided config always wins.

## Steps

- [x] Add `kubernetes_version` field to `Settings`
- [x] Add Kubernetes detection + URL construction to `schema.rs` (77c4298)
- [ ] Integrate into `parse_and_publish` as third fallback
- [ ] Write tests for detection and URL construction
- [ ] Update `configuration.md` with new setting
- [ ] Update `feature-log.md`

## Tasks

### Task 1: Kubernetes detection and URL construction

Add a function to `schema.rs` that takes a YAML text
string (or parsed documents) and extracts root-level
`apiVersion` and `kind` values. Add a second function
that constructs the schema URL from those values plus
the Kubernetes version string.

Detection rules:
- Only inspect the **first** YAML document in multi-doc
  files (Kubernetes manifests are single-document).
- Both `apiVersion` and `kind` must be present as
  root-level scalar string values.
- `apiVersion` format: either `v1` (core API) or
  `{group}/{version}` (grouped API).
- URL construction: lowercase the `kind`, then:
  - Core: `{kind}-{apiVersion}.json`
  - Grouped: `{kind}-{group}-{version}.json`
- Full URL template:
  `https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v{k8s_version}-standalone-strict/{filename}`

Files: `rlsp-yaml/src/schema.rs`

- [ ] `detect_kubernetes_resource(text: &str, docs: &[YamlOwned]) -> Option<(String, String)>`
      Returns `(api_version, kind)` if the first document
      has both fields at the root level.
- [ ] `kubernetes_schema_url(api_version: &str, kind: &str, k8s_version: &str) -> String`
      Constructs the full GitHub raw URL.
- [ ] Unit tests for both functions covering:
  - Core API (v1 Pod)
  - Grouped API (apps/v1 Deployment)
  - HPA autoscaling/v2 case (the motivating issue)
  - Missing apiVersion or kind → None
  - Multi-document: only first doc inspected
  - Non-string apiVersion/kind values → None

### Task 2: Settings and server integration

Add the `kubernetesVersion` setting and wire the
Kubernetes auto-detection into the schema resolution
pipeline.

Files: `rlsp-yaml/src/server.rs`

- [ ] Add `kubernetes_version: Option<String>` to
      `Settings` (serde default: `None`, meaning
      use built-in default `"1.32.0"`)
- [ ] Add `get_kubernetes_version()` helper on `Backend`
- [ ] In `parse_and_publish`, after the "no modeline,
      no glob match" branch, call
      `detect_kubernetes_resource` and if found, construct
      the URL via `kubernetes_schema_url` and pass it to
      `process_schema`.
- [ ] Integration test: verify that a Kubernetes manifest
      without modeline or glob triggers schema resolution.

### Task 3: Documentation

Update configuration and feature log docs to reflect the
new capability.

Files: `rlsp-yaml/docs/configuration.md`,
`rlsp-yaml/docs/feature-log.md`

- [ ] Add `kubernetesVersion` setting to configuration.md
- [ ] Document auto-detection behavior and priority
- [ ] Add feature entry to feature-log.md

## Decisions

- **Schema variant:** `-standalone-strict` — includes
  `additionalProperties: false`, which catches unknown
  properties. This is the whole point of the feature
  (detecting wrong-version fields).
- **Default K8s version:** `1.32.0` — a recent stable
  release with wide adoption. Users can override via
  `kubernetesVersion` setting.
- **Detection scope:** First document only — Kubernetes
  manifests are single-document YAML. Multi-document files
  (e.g. `---` separated) could have mixed content; only
  the first doc is inspected for safety.
- **No CRD support:** This feature covers standard
  Kubernetes resources only. CRDs would require fetching
  schemas from a running cluster, which is out of scope.
