# Vendored JSON Schemas

Unmodified copies of real, self-contained schemas (no remote `$ref`). The step-budget
tests in `src/schema_validation/budget_tests.rs` validate real-world documents against
them to size the validation step budget. The directory is excluded from the published
crate package (`exclude` in `Cargo.toml`).

| File | Source (pinned commit) | Licence |
|---|---|---|
| `cloud-init.json` | https://raw.githubusercontent.com/canonical/cloud-init/d5873d2e693cb3e83d7e50c97da3b8586faf8c2e/cloudinit/config/schemas/schema-cloud-config-v1.json | GPL-3.0 or Apache-2.0 (used under Apache-2.0) |
| `github-workflow.json` | https://raw.githubusercontent.com/SchemaStore/schemastore/57967c6f6d744228bdcd0500703d345bb8334574/src/schemas/json/github-workflow.json | Apache-2.0 |
| `compose-spec.json` | https://raw.githubusercontent.com/compose-spec/compose-go/52eea9f323cbdb2bf9c2092513615771faec4690/schema/compose-spec.json | Apache-2.0 |
| `kubernetes-deployment-apps-v1.json` | https://raw.githubusercontent.com/yannh/kubernetes-json-schema/f7d855e49c34b039c9ef2196444910275e31e09e/master-standalone-strict/deployment-apps-v1.json | Apache-2.0 |
