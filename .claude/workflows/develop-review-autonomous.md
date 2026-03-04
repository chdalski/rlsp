# Develop-Review (Autonomous)

<!-- SYNC NOTE: The Flow section (steps 1–14) is identical to
     develop-review-supervised.md. When editing the dev-team
     flow, apply the same change to the other file. Only the
     Commit section (step 15) differs between the two. -->

## When to Use

Use this workflow for tasks that produce code — features,
bug fixes, refactors, or any change that touches source
files and tests. It provides a full development cycle with
test-list-driven development, security review, and
independent quality review before each commit. There are no
user checkpoints between Reviewer approval and commit — the
user trusts the agent quality gates (Test Engineer sign-off,
Security Engineer sign-off, Reviewer approval) to ensure
correctness. This is the right choice when the user wants
speed and trusts the team to batch work autonomously.

For the same workflow with a user checkpoint before each
commit, see `develop-review-supervised.md` — the user
approves every change before it enters git history.

Not appropriate for non-code tasks — use Direct-Review
for documentation or configuration changes.

## Agents

### Workflow-Specific

| Agent | Role |
|-------|------|
| **Architect** | Reads the codebase, writes plans, decomposes into task slices, and feeds tasks to the dev-team sequentially. Collects completion signals and sequences the next task. |
| **Developer** | Implements all code (source + tests). Owns every code file. Uses WebSearch/WebFetch for API docs and library examples. |
| **Test Engineer** | Advisory — designs test specifications (the test list), verifies Developer's tests match the spec before and after implementation. Does not write code. |
| **Security Engineer** | Advisory — assesses security implications, flags vulnerabilities, provides pre- and post-implementation sign-offs. Does not write code. |
| **Reviewer** | Independent quality gate — evaluates correctness, security, test coverage, design, and idioms. Composes the commit message and commits approved work immediately after approval. |

## Team Lifecycle

The lead creates one team via `TeamCreate` with all
workflow agents at workflow start. The team persists
across all task slices — re-spawning per task incurs
startup cost and breaks `SendMessage` communication.

## Flow

### Per Task Slice

1. **Architect sends task** to Developer, Test Engineer,
   and Security Engineer simultaneously — all three need
   the full task context to discuss the approach.

2. **Dev-team discusses the task.** Security Engineer
   broadcasts a pre-implementation security assessment to
   Developer and Test Engineer — OWASP categories, what
   the Test Engineer should cover, what the Developer
   should watch for.

3. **Test Engineer produces the test list** — a structured
   specification of every test case — and sends it to the
   Developer. This is the contract for what gets tested.

4. **Developer writes all tests** from the test list in a
   single batch — unit tests and integration tests together.
   If integration tests are included, the Developer spikes
   one integration test first to validate the test harness
   (server setup, database fixtures, framework test
   utilities) before writing the rest. Writing all tests at
   once gives a complete picture of expected behavior before
   implementation, which leads to better design decisions.
   Sends completed tests to the Test Engineer.

5. **Test Engineer verifies tests** — reads all test files,
   compares against the test list. For each test, checks
   that name, scenario, and assertions match the
   specification. If tests are missing or incorrect, tells
   the Developer what to fix and waits for corrections.
   When satisfied, sends "tests verified" to Developer.
   Developer does not start implementing source code until
   this message arrives — this checkpoint catches
   spec-to-test gaps early, before implementation effort
   is spent on a misunderstood specification.

6. **Developer implements source code** to make all tests
   pass. Follows the rule system's guidance (language
   idioms, code principles, simplicity) that loads
   automatically based on files touched.

7. **Developer reports implementation complete** to Test
   Engineer and Security Engineer. Both must provide their
   sign-offs before the task can proceed.

8. **Test Engineer reads all test files again** — confirms
   tests were not skipped, weakened, or removed during
   implementation. Sends **test sign-off** to Developer.

9. **Security Engineer reviews the Developer's code** —
   checks for vulnerabilities, missing input validation,
   auth gaps. Sends **security sign-off** to Developer.

10. **Developer reports implementation complete** to
    Architect — having received both sign-offs, sends a
    summary via SendMessage.

11. **Architect notifies lead** that the task is ready for
    review.

### Review

12. **Lead sends to Reviewer.** Reviewer evaluates
    correctness, security, test coverage, design, and
    language idioms.

13. **If rejected:** Reviewer sends specific findings to
    Developer, Test Engineer, and Security Engineer. All
    three receive findings so they can coordinate the fix.
    Developer fixes. Return to step 7 — both sign-offs
    are required again after fixes, because changes during
    a fix can introduce new issues.

14. **If approved:** Reviewer reports approval to lead
    with review summary, proposed commit message, and
    file list.

### Commit

15. **Lead immediately tells Reviewer to commit.**
    Reviewer stages the files and commits with the
    prepared message, reports the short SHA to the lead.
    Lead tells Architect the task is committed. Architect
    marks the task completed via TaskUpdate, updates the
    plan file, and feeds the next task slice (loop to
    step 1).

## Completion Criteria

The workflow is complete when:

- All task slices from the Architect's plan are committed
- Each slice received both Test Engineer and Security
  Engineer sign-offs before review
- Each slice passed Reviewer approval before commit
- All tests pass across the full project after the final
  commit
