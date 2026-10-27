**Repository:** root
**Status:** InProgress
**Created:** 2026-10-27

# Validate the Inputs of the Kubernetes Schema URL

## Goal

Kubernetes auto-detection builds its schema URL from `kind` and
`apiVersion` taken unchecked from the YAML document and from the
`kubernetesVersion` setting, so a manifest with
`kind: ../../../../attacker/repo/main/evil` makes the server load
an arbitrary schema from raw.githubusercontent.com, silently.
Accept only values that follow Kubernetes naming rules, fall back
to `master` with a Warning for an invalid `kubernetesVersion` and
flag such a value in the VS Code settings, and build the file names
the schema repository actually uses, so that auto-validation also
works for API groups such as `networking.k8s.io` and
`rbac.authorization.k8s.io`; custom resources, for which the
repository has no schemas, go on to SchemaStore instead of a failed
fetch.

## Context

- `rlsp-yaml/src/schema/association.rs`: `detect_kubernetes_resource`
  returns the root mapping's `apiVersion` and `kind` as raw strings;
  `kubernetes_schema_url` formats them into
  `https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/{dir}/{file}`
  with `dir` = `master-standalone-strict` or
  `v{kubernetesVersion}-standalone-strict`. Both functions are public
  through `rlsp_yaml::schema`.
- `rlsp-yaml/src/server.rs`: the schema source chain is modeline,
  then `schemas` globs, then Kubernetes auto-detection, then
  SchemaStore. Settings are deserialized in `initialize` and
  `did_change_configuration`; `kubernetesVersion` is
  `Settings::kubernetes_version: Option<String>` read by
  `get_kubernetes_version`.
- The `kubernetesVersion` setting is declared in
  `rlsp-yaml/integrations/vscode/package.json` and documented in
  `rlsp-yaml/docs/configuration.md`.
- `tests/lsp_lifecycle/` has `kubernetes_detection.rs` and a
  `FakeProxy` (`fake_proxy.rs`) that records CONNECT requests.
- References: Kubernetes object names
  (https://kubernetes.io/docs/concepts/overview/working-with-objects/names/),
  API versioning
  (https://kubernetes.io/docs/reference/using-api/#api-versioning),
  the schema repository
  (https://github.com/yannh/kubernetes-json-schema).

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: settings are stored through one method
- [ ] Task 2: only Kubernetes names reach the schema URL
- [ ] Task 3: file names match the schema repository; custom
      resources are not fetched
- [ ] Task 4: an invalid `kubernetesVersion` falls back to `master`
      with a Warning
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Store settings through one method

`initialize` and `did_change_configuration` each store the
deserialized settings themselves. One method stores them for both,
so that Task 4 adds its Warning in one place. Behavior is
unchanged.

- [x] Both handlers store settings through one shared method, and
      no other production code path stores them
- [x] `tests/lsp_lifecycle/kubernetes_detection.rs`,
      `tests/lsp_lifecycle/configuration.rs`, and the
      `did_change_configuration` tests in
      `tests/lsp_lifecycle/validators_integration.rs` pass with
      their assertions unchanged
- [x] Build, tests, clippy, and fmt pass

### Task 2: Accept only Kubernetes names in the schema URL

Detection yields a resource only when its values follow Kubernetes
naming rules, and the URL builder takes only such a validated
resource, never raw document text: `kind` starts with an ASCII
letter, continues with ASCII letters and digits, and has at most
63 characters; a group is a DNS-1123 subdomain (lowercase
alphanumeric labels of 1–63 characters, inner `-`, joined by `.`,
at most 253 characters); a version is `v` followed by digits,
optionally followed by `alpha` or `beta` and digits.

- [ ] Detection returns a resource only for values that follow
      these rules; `kubernetes_schema_url` accepts only that
      validated resource
- [ ] A manifest whose `kind` or `apiVersion` breaks the rules —
      `../` sequences, a `/` in the version, an uppercase group, an
      empty or over-long value among them — triggers no Kubernetes
      fetch, and the document goes on to SchemaStore matching; an
      integration test in `rlsp-yaml/tests/lsp_lifecycle/` pins the
      absent fetch for the traversal manifest
- [ ] A property test shows that, for a fixed valid
      `kubernetesVersion`, every accepted resource yields a URL that,
      after `validate_and_normalize_url`, starts with
      `https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/`
- [ ] The existing detection and URL tests pass with their
      assertions unchanged — only edits for the validated type
- [ ] The "Schema Resolution Priority" section of
      `rlsp-yaml/docs/configuration.md` tells the reader that
      auto-detection needs a valid Kubernetes `apiVersion` and
      `kind` and that other documents go on to SchemaStore
- [ ] Build, tests, clippy, and fmt pass

### Task 3: Build the repository's file names and skip custom resources

The schema repository names a grouped file
`{kind}-{segment}-{version}.json`; the core group stays
`{kind}-{version}.json`. It holds schemas only for Kubernetes's own
API groups: every group without dots (`apps`, `autoscaling`,
`batch`, `policy`, `extensions`; a custom-resource group always
contains a dot), whose segment is the group itself, and these
dotted groups, whose segment is their first label except
`apiserverinternal` for `internal.apiserver.k8s.io`:
`admissionregistration.k8s.io`, `apiextensions.k8s.io`,
`apiregistration.k8s.io`, `authentication.k8s.io`,
`authorization.k8s.io`, `certificates.k8s.io`,
`coordination.k8s.io`, `discovery.k8s.io`, `events.k8s.io`,
`flowcontrol.apiserver.k8s.io`, `internal.apiserver.k8s.io`,
`lifecycle.k8s.io`, `networking.k8s.io`, `node.k8s.io`,
`rbac.authorization.k8s.io`, `resource.k8s.io`,
`scheduling.k8s.io`, `storage.k8s.io`, `storagemigration.k8s.io`
(from the group/version/kind entries of its `master/_definitions.json`
at commit f7d855e49c34b039c9ef2196444910275e31e09e). Every other
dotted group is a custom resource — including groups ending in
`.k8s.io` such as `kustomize.config.k8s.io`,
`gateway.networking.k8s.io`, and `snapshot.storage.k8s.io`. Verified
file names in its `master-standalone-strict` directory at that
commit include
`ingress-networking-v1.json`, `networkpolicy-networking-v1.json`,
`clusterrole-rbac-v1.json`,
`certificatesigningrequest-certificates-v1.json`,
`validatingwebhookconfiguration-admissionregistration-v1.json`,
`apiservice-apiregistration-v1.json`,
`flowschema-flowcontrol-v1.json`, and
`storageversion-apiserverinternal-v1alpha1.json`.

- [ ] URL tests pin each verified file name above plus the core
      (`pod-v1.json`) and dotless-group (`deployment-apps-v1.json`)
      cases
- [ ] Manifests of the custom-resource groups `cert-manager.io/v1`
      and `kustomize.config.k8s.io/v1beta1` trigger no Kubernetes
      fetch and go on to SchemaStore matching, pinned by an
      integration test; each listed dotted group maps to its segment
      in a unit test
- [ ] `should_not_warn_when_kubernetes_schema_fetch_fails` keeps
      asserting that a failed Kubernetes fetch stays silent, now
      with a built-in group whose file does not exist
- [ ] `rlsp-yaml/docs/configuration.md` tells the reader that
      auto-detection covers built-in Kubernetes API groups and that
      custom resources go on to SchemaStore
- [ ] `rlsp-yaml/docs/feature-log.md` has an entry that tells the
      reader how Kubernetes auto-detection now treats document
      names, Kubernetes's own API groups, and custom resources
- [ ] Build, tests, clippy, and fmt pass

### Task 4: Fall back to `master` for an invalid `kubernetesVersion`

`kubernetesVersion` is valid when it is `master` (case-sensitive)
or three dot-separated groups of ASCII digits such as `1.29.0`. The
setting keeps its raw string; the value is interpreted when it is
read, so an invalid value cannot make the other settings fail to
load. The Warning text goes through `printable_log_message`, like
the other client log messages. tower-lsp 0.20's
`Client::log_message` is sent without the initialization check, so
a Warning sent while handling `initialize` reaches the client; the
LSP specification allows `window/logMessage` during the
`initialize` request.

- [ ] An absent or invalid value selects `master`; the URL builder
      accepts only the validated version
- [ ] Applying settings with an invalid value — at initialization
      or on a configuration change — sends exactly one Warning to
      the client log that names the setting and the rejected
      value, sanitized like the other client log messages; a valid
      value sends none; integration tests in
      `rlsp-yaml/tests/lsp_lifecycle/` pin both paths, count the
      Warnings across `initialize`, `initialized`, and the first
      opened document, and check that the resulting schema URL
      uses `master`
- [ ] `should_treat_capitalised_master_as_versioned_prefix` now
      asserts that `Master` is invalid and selects `master`
- [ ] The VS Code setting `rlsp-yaml.kubernetesVersion` carries a
      pattern that accepts exactly the values the server accepts,
      and an error message; the extension's build, tests, lint,
      format check, and typecheck pass
- [ ] The `kubernetesVersion` section of
      `rlsp-yaml/docs/configuration.md` tells the reader which
      values are accepted and what happens with any other value,
      and the feature-log entry for Kubernetes auto-detection also
      tells the reader about the `kubernetesVersion` fallback
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Schemas for custom resources from another catalog
- Checking schema URLs against DNS resolution (SSRF hardening)
- Validating other settings

## Decisions

- **Validate against Kubernetes naming rules:** a name that breaks
  them is not a Kubernetes resource, and rejecting it also saves a
  fetch. Declined: percent-encoding the values, which closes the
  traversal but still fetches for every value.
- **Validated types:** detection yields a validated resource and
  the version a validated value, so the URL builder cannot receive
  raw text.
- **Raw string in `Settings`:** a validating field type would make
  the whole settings object fail to deserialize, resetting every
  setting at initialization and dropping a whole configuration
  change.
- **Invalid names go on to SchemaStore:** a document with an
  invalid `kind` is not a Kubernetes manifest, and the source chain
  stays "first source that yields a URL wins".
- **Built-in groups from an explicit list:** the repository holds
  no custom-resource schemas, and neither a first-label rule nor a
  `.k8s.io` suffix rule tells them apart — `apps.kruise.io` would
  hit a built-in schema and report false errors, and
  `kustomize.config.k8s.io` would fail a fetch instead of reaching
  SchemaStore's Kustomization schema. A new Kubernetes API group
  needs a list entry before it is auto-detected.
- **`internal.apiserver.k8s.io` is an exception:** the repository
  names files after the API's Go package, which equals the group's
  first label except for this group.
- **Invalid `kubernetesVersion` falls back to `master` with a
  Warning:** declined: disabling auto-detection, or not validating
  the setting.
- **Warning on every application of invalid settings:** clients
  send all settings on any change, so the Warning repeats on each
  configuration change while the value stays invalid; it is never
  sent per document.
- **Auto-detection stays silent on fetch failures:** unchanged; its
  URL is a guess.
- **The initialization Warning is sent while handling
  `initialize`:** declined review finding that it would be dropped
  there — `Client::log_message` bypasses tower-lsp's initialization
  check, and the LSP specification allows `window/logMessage`
  during `initialize`; so one store method can send the Warning for
  both paths.
