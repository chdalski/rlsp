# Advisor Gate Independence

When a task requires consultation with multiple advisors,
each advisor's gates are independent. One advisor's
deliverables never satisfy another advisor's gate.

## The Rule

Every advisor consultation has its own input gate
(guidance before implementing) and its own output gate
(sign-off on the completed work). The gates are scoped
per advisor:

- **Test advisor input gate** — a test list authored by
  the test advisor, informed by their testability
  expertise.
- **Test advisor output gate** — the test advisor verifies
  the completed test set against their test list.
- **Security advisor input gate** — a risk assessment
  authored by the security advisor, from their
  trust-boundary expertise.
- **Security advisor output gate** — the security advisor
  signs off on the implementation's treatment of the
  identified risks.

A task requiring both advisors has four distinct gates.
Each gate must be satisfied by its own advisor.

## What Does Not Count

- **One advisor's deliverable containing content that
  looks like another advisor's output.** A security
  assessment that lists test scenarios does not satisfy
  the test advisor's input gate — those scenarios are
  security probes, not a coverage-driven test plan.
- **An unsolicited advisor message** in the developer's
  inbox. If the developer did not consult the advisor,
  the message is not a gate response — it is
  informational context. The developer must still send
  an explicit consult request to satisfy the gate.
- **A prior session's advisor output.** New team cycles
  start fresh. Prior guidance does not carry forward
  unless the requester explicitly passes it in the task
  dispatch.

## How to Apply

### For developers

Consult each named advisor separately, both at the input
gate (before implementing) and at the output gate (before
submitting to the reviewer). One `SendMessage` per advisor,
naming the specific deliverable expected
("test-engineer: please provide a test list for <task>";
"security-engineer: please provide a risk assessment for
<task>"). Do not use one advisor's deliverable to cover
another advisor's gate, even if the content overlaps.

### For reviewers

At review time, verify the handoff explicitly cites each
required advisor's gates. If the task required two
advisors, the handoff must show four gate citations. A
handoff that cites three and claims the fourth "was
covered by the other advisor" is grounds for rejection —
send it back for the missing explicit sign-off.

## Why This Matters

Each advisor sees the task from a different expert lens.
The test engineer thinks about coverage and edge cases;
the security engineer thinks about trust boundaries and
failure modes. Their deliverables are not interchangeable
even when they intersect on content.

A production incident showed the cost: the developer
skipped the test advisor's input gate because a security
assessment happened to list test scenarios. When the test
advisor was finally consulted at the output gate, they
immediately identified two missing test cases that a
proper input-gate consult would have caught earlier — one
round-trip in the review pipeline wasted, and the process
violation needed retroactive documentation in the plan.

## Related Rules

- `risk-assessment.md` determines which advisors a task
  requires. Gate independence operates downstream: once
  risk-assessment says "both advisors required," the
  gates are independent.
- `procedural-fidelity.md` — each gate is a numbered step;
  skipping one because another "covers it" is a
  sufficiency fallacy.
- The lead's `CLAUDE.md` dispatch instructions explain how
  both gates are named explicitly in the task message
  (input gate before implementing, output gate before
  submitting to the review agent). This rule is the
  behavioral counterpart — the lead names the gates, and
  developers/reviewers honor them independently.
