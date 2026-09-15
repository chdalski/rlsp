---
name: No mechanical code splits from line counts
description: Do not split code (especially tests from production) just to hit arbitrary line-count thresholds — splits must have structural justification
type: feedback
---

Do not use fixed line counts as hard acceptance criteria that
dictate code structure. Splitting test code into its own
submodule file just to satisfy a "≤ N lines" target is
busywork with no readability or logic value.

**Why:** A production incident had the developer create
`yaml11_bool/tests.rs` and `flow_to_block/tests.rs`
submodules solely to bring parent files under an 800-line
limit. The user called this out as something "only an idiot
would do" — tests belong with the code they test, and
decisions to split must have readability, functionality, or
logical value.

**How to apply:** When planning, use line counts as
diagnostic signals ("this file is large, investigate") not
prescriptive rules ("this file is large, split it"). Before
proposing a split, articulate the structural reason — if
you can only cite a number, the split is not justified.
Never separate test code from production code solely for
line-count compliance.
