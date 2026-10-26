# Vendored JSON Schemas

Unmodified copies of real, self-contained schemas (no remote `$ref`). The step-budget
tests in `src/schema_validation/budget_tests.rs` validate real-world documents against
them to size the validation step budget. The parser tests in `src/schema.rs` check that a
parsed schema is no larger than its source, and the `tests/lsp_lifecycle/` suites use the
draft-07 meta-schema as a self-referencing schema served over HTTP. The directory is
excluded from the published crate package (`exclude` in `Cargo.toml`).

| File | Source (pinned commit) | Licence |
|---|---|---|
| `cloud-init.json` | https://raw.githubusercontent.com/canonical/cloud-init/d5873d2e693cb3e83d7e50c97da3b8586faf8c2e/cloudinit/config/schemas/schema-cloud-config-v1.json | GPL-3.0 or Apache-2.0 (used under Apache-2.0) |
| `github-workflow.json` | https://raw.githubusercontent.com/SchemaStore/schemastore/57967c6f6d744228bdcd0500703d345bb8334574/src/schemas/json/github-workflow.json | Apache-2.0 |
| `compose-spec.json` | https://raw.githubusercontent.com/compose-spec/compose-go/52eea9f323cbdb2bf9c2092513615771faec4690/schema/compose-spec.json | Apache-2.0 |
| `kubernetes-deployment-apps-v1.json` | https://raw.githubusercontent.com/yannh/kubernetes-json-schema/f7d855e49c34b039c9ef2196444910275e31e09e/master-standalone-strict/deployment-apps-v1.json | Apache-2.0 |
| `json-schema-draft-07.json` | https://raw.githubusercontent.com/json-schema-org/json-schema-spec/20a3fee852db88519dcb3bb329b7a872ebad8953/schema.json | AFL or BSD by the repository (used under BSD-3-Clause) |

## Notice for `json-schema-draft-07.json`

The json-schema-spec repository states that its source material is licensed under the AFL or
BSD license (README, "License"). The file is used under the BSD 3-clause licence, whose text,
with the copyright notice, the conditions and the disclaimer, is retained here from the
repository's `LICENSE` file
(https://raw.githubusercontent.com/json-schema-org/json-schema-spec/main/LICENSE):

> Copyright (c) 2022 JSON Schema Specification Authors
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions are
> met:
>
> 1. Redistributions of source code must retain the above copyright
> notice, this list of conditions and the following disclaimer.
>
> 2. Redistributions in binary form must reproduce the above copyright
> notice, this list of conditions and the following disclaimer in the
> documentation and/or other materials provided with the distribution.
>
> 3. Neither the name of the copyright holder nor the names of its
> contributors may be used to endorse or promote products derived from
> this software without specific prior written permission.
>
> THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
> "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
> LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
> A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
> HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
> SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
> TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
> PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
> LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
> NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
> SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
