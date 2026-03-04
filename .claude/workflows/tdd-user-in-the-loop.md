# TDD User-in-the-Loop

## When to Use

Use this workflow when the user wants fine-grained control
over the development process — specifically, visibility and
approval at every phase of test-driven development
(Red-Green-Refactor). It is appropriate when:

- The user wants to be in the loop when tests are created
  and when code is written to satisfy them
- The task benefits from incremental, disciplined TDD where
  each test is written, failed, passed, and refactored
  before moving to the next
- The user values correctness confidence over speed — this
  workflow is slower than the Develop-Review variants
  because it stops for user approval at every phase
  transition
- The codebase is unfamiliar or the task involves subtle
  logic where assumptions need frequent validation

Not appropriate when speed is the priority, when the user
trusts the team to batch work autonomously, or for non-code
tasks. Use Develop-Review (Supervised or Autonomous) instead
when the user does not need per-phase control — they produce
the same quality with fewer interruptions.

## Agents

### Workflow-Specific

| Agent | Role |
|-------|------|
| **Architect** | Reads the codebase, writes plans, decomposes into task slices, and feeds tasks to the dev-team sequentially. Collects completion signals and sequences the next task. |
| **Developer** | Implements all code (source + tests). Activates one test at a time from the test list and executes the Red-Green-Refactor cycle for each. |
| **Test Engineer** | Advisory — designs the full test list upfront, verifies each test as the Developer writes it, and provides post-implementation sign-off after all cycles complete. Does not write code. |
| **Security Engineer** | Advisory — provides pre-implementation security assessment and post-implementation sign-off. Does not write code. |
| **Reviewer** | Independent quality gate — evaluates the completed task for correctness, security, test coverage, design, and idioms. Composes the commit message and commits approved work after the user checkpoint. |

## Team Lifecycle

The lead creates one team via `TeamCreate` with all
workflow agents at workflow start. The team persists
across all task slices — re-spawning per task incurs
startup cost and breaks `SendMessage` communication.

## Flow

### Per Task Slice

#### Setup

1. **Architect sends task** to Developer, Test Engineer,
   and Security Engineer simultaneously — all three need
   the full task context to discuss the approach.

2. **Dev-team discusses the task.** Security Engineer
   broadcasts a pre-implementation security assessment to
   Developer and Test Engineer — OWASP categories, what
   the Test Engineer should cover, what the Developer
   should watch for.

3. **Test Engineer produces the test list** — a structured
   specification of every test case, ordered from simple
   to complex — and sends it to the Developer and the
   lead. The ordering matters because TDD builds
   complexity incrementally; simple tests drive out the
   core design before edge cases add conditional logic.

4. **User checkpoint — test list approval.** The lead
   presents the test list to the user. The user may
   approve, modify, add, or remove test cases. The lead
   relays any changes to the Test Engineer and Developer.
   This checkpoint exists because the user explicitly
   wants to be in the loop when tests are created — the
   test list defines what the code will do, so approving
   it is approving the specification.

#### TDD Cycles (steps 5–11, repeated per test)

The Developer works through the approved test list one
test at a time. For each test case:

5. **Red — Developer writes one test.** The Developer
   writes the next test from the approved list. The test
   must fail when run — this confirms the test actually
   tests something and the behavior does not already
   exist. The Developer runs the test, confirms failure,
   and sends the test code and failure output to the
   lead.

   **Failed prediction:** If the test passes unexpectedly
   (the behavior already exists), the Developer stops
   and notifies the lead immediately. The lead consults
   the user — this may indicate the test list needs
   updating, the behavior was already implemented in a
   prior cycle, or the test is not asserting what was
   intended. Do not proceed until the user decides how
   to handle it.

6. **User checkpoint — Red phase.** The lead presents the
   failing test and its output to the user. The user
   confirms the test is correct and approves moving to
   the Green phase. This checkpoint lets the user verify
   that the test matches their intent before any
   implementation happens.

7. **Green — Developer writes minimal implementation.**
   The Developer writes the minimum code needed to make
   the failing test pass — no more. All existing tests
   must also continue to pass. "Minimal" means the
   simplest code that satisfies the assertion, even if
   it looks naive. Premature generalization at this stage
   leads to implementations that serve hypothetical cases
   rather than actual test requirements. The Developer
   runs all tests, confirms they pass, and sends the
   implementation and test output to the lead.

   **Failed prediction:** If any previously passing test
   now fails, the Developer stops and notifies the lead.
   The lead consults the user — the new implementation
   broke an assumption from an earlier cycle. Do not
   proceed until the regression is resolved.

8. **User checkpoint — Green phase.** The lead presents
   the implementation and passing test output to the
   user. The user confirms the implementation is
   acceptable and approves moving to the Refactor phase.

9. **Refactor — Developer improves the code.** The
   Developer must attempt at least one refactoring.
   Evaluate naming first, then look for duplication,
   structural improvements, and simplification
   opportunities. Report what was changed and why, or
   if a refactoring was attempted and rejected, explain
   why it would have made the code worse. Mandatory
   refactoring after every Green phase is core TDD
   discipline — skipping it lets design debt accumulate
   across cycles until the code becomes difficult to
   extend. The Developer runs all tests after
   refactoring to confirm they still pass, and sends
   the refactored code and test output to the lead.

   **Failed prediction:** If any test fails after
   refactoring, the Developer reverts the refactoring
   change and notifies the lead. Refactoring must not
   change behavior — a failing test means the
   refactoring was incorrect.

10. **User checkpoint — Refactor phase.** The lead
    presents the refactored code and passing test output
    to the user. The user approves moving to the next
    test.

11. **Test Engineer verifies the test.** After user
    approval of the cycle, the Test Engineer reads the
    test file, confirms the test matches its
    specification from the test list (name, scenario,
    assertions), and sends confirmation to the Developer.
    This incremental verification catches spec drift
    early — without it, mismatches accumulate across
    cycles and require a costly batch correction at the
    end.

**Repeat steps 5–11** for each test case in the approved
test list.

#### Sign-offs

12. **Test Engineer post-implementation sign-off.** After
    all TDD cycles are complete, the Test Engineer reads
    all test files and confirms: every test from the
    approved list exists, no tests were skipped or
    weakened, and all tests pass. Sends sign-off to
    Developer.

13. **Security Engineer post-implementation sign-off.**
    The Security Engineer reviews the complete
    implementation — all source and test files written
    during the TDD cycles. Checks for vulnerabilities,
    missing input validation, auth gaps. Sends sign-off
    to Developer.

14. **Developer reports implementation complete** to
    Architect — having received both sign-offs, sends a
    summary via SendMessage.

15. **Architect notifies lead** that the task is ready
    for review.

### Review

16. **Lead sends to Reviewer.** Reviewer evaluates
    correctness, security, test coverage, design, and
    language idioms.

17. **If rejected:** Reviewer sends specific findings to
    Developer, Test Engineer, and Security Engineer. All
    three receive findings so they can coordinate the
    fix. Developer fixes. Return to step 12 — both
    sign-offs are required again after fixes, because
    changes during a fix can introduce new issues.

18. **If approved:** Reviewer reports approval to lead
    with review summary, proposed commit message, and
    file list.

### Commit

19. **User checkpoint — commit approval.** The lead
    presents the completed work, Reviewer's summary, and
    proposed commit message to the user. Even though the
    user approved each phase individually, this final
    checkpoint covers the aggregate — the user sees the
    full changeset before it enters git history.

20. **Lead tells Reviewer to commit.** Reviewer stages
    the files and commits with the prepared message,
    reports the short SHA to the lead. Lead tells Architect
    the task is committed. Architect marks the task
    completed via TaskUpdate, updates the plan file, and
    feeds the next task slice (loop to step 1).

## Completion Criteria

The workflow is complete when:

- All task slices from the Architect's plan are committed
- Each test in every test list went through a complete
  Red-Green-Refactor cycle with user approval at each
  phase transition
- Refactoring was attempted after every Green phase
- Each slice received both Test Engineer and Security
  Engineer sign-offs before review
- Each slice passed Reviewer approval before commit
- All tests pass across the full project after the final
  commit
- The user approved each commit at the commit checkpoint
