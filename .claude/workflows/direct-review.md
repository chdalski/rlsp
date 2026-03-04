# Direct-Review

## When to Use

Use this workflow for tasks where the user prefers
directness over process, and the change meets both criteria:

1. **No security ramifications** — the change does not
   touch auth, input validation, cryptography, access
   control, or data handling that could introduce
   vulnerabilities
2. **Tests already cover it, or no tests are needed** —
   existing tests validate the behavior being changed,
   or the change is non-behavioral (typo, comment,
   documentation, config value)

Examples: fixing a typo, updating a config value,
renaming a variable, adjusting documentation.

Not appropriate when either criterion is not met — those
benefit from specialized agents (Test Engineer, Security
Engineer) that catch issues a single perspective would
miss.

## Agents

- **Reviewer** — independent quality gate, including
  CLAUDE.md drift detection. Created via `TeamCreate` as a
  one-agent team so it can receive the commit signal
  after the user checkpoint. The Reviewer composes the
  commit message and commits approved work.

Keeping the Reviewer alive across the user checkpoint is
why TeamCreate is used even for this single-agent case —
it allows the lead to send a "commit" message after user
approval rather than re-spawning a new agent.

## Flow

1. Lead reads relevant files to understand current state
2. Lead implements the change directly
3. Lead runs tests and linters if applicable — catching
   regressions before presenting to the Reviewer avoids
   wasted review cycles
4. Lead creates a one-agent team via TeamCreate with the
   Reviewer — Reviewer performs full review including
   CLAUDE.md drift detection. Even small changes can
   introduce drift
   (e.g., renaming a directory that CLAUDE.md references),
   and Direct-Review has the least process, making
   undetected drift most likely here.
5. If rejected: lead fixes issues and re-sends to Reviewer
6. **User checkpoint** — lead presents the completed work,
   Reviewer's summary, and proposed commit message for
   user approval
7. If approved, lead tells Reviewer to commit
8. If changes are needed, lead adjusts and returns to
   step 3

## Completion Criteria

- Change implemented as requested
- Tests pass (if applicable)
- Reviewer has approved the result (including drift check)
- User has approved the result
- Work committed by Reviewer
