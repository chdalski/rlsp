---
name: yaml-test-suite corpus is mandatory coverage
description: Any behavior or invariant that can be exercised against the yaml-test-suite corpus MUST be tested against it — synthetic-only fixtures are not an acceptable substitute
type: feedback
---
Every test target in the rlsp-yaml workspace that can plausibly be
exercised against the yaml-test-suite corpus MUST be exercised
against it. This is the user's explicit law for the team —
"everything that can be tested against the yaml-test-suite needs
to be tested against it." No compromises, ever.

**Why:** the user is explicit that the yaml-test-suite corpus is
the project's ground truth for YAML semantics. A parser crate
with "a conformance test suite" that actually tests a handful of
hand-crafted documents silently weakens coverage. Synthetic
fixtures can only express what the author already thought of;
the yaml-test-suite corpus surfaces edge cases the author did
not anticipate — exactly the bugs the team cares about.

**How to apply:**

- When writing an invariant test, a coverage check, or any
  assertion of the form "for every valid YAML, property P
  holds," the input set is the yaml-test-suite corpus. A
  synthetic document is a supplement, never a substitute.
- When the test advisor provides a test list that names the
  corpus as the input set, the implementor MUST use the corpus.
  A single hand-crafted document is a rejection-worthy
  deviation from the test plan — the test advisor should flag
  it at the output gate, and the lead should back the advisor
  on rejection.
- When phrasing acceptance criteria as lead, write "iterates
  the yaml-test-suite corpus" explicitly. "Conformance corpus
  inputs" is ambiguous and has been misread as "any document
  that exercises conformance."
- This rule applies to all testable surfaces where the suite is
  relevant: parser event stream, AST construction, round-trip
  properties, validator invariants, formatter round-trips.
  Purely internal utilities with no YAML input surface are the
  only exemption.
