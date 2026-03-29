# Risk and Uncertainty Assessment

When deciding whether to consult advisors before
implementing a task, assess its risk and uncertainty using
the indicators below. This framework ensures advisors are
consulted when their expertise adds value, and skipped when
the task is straightforward — mandatory consultation on
every task produces rubber-stamp sign-offs that dilute the
signal when real issues arise.

## When to Consult the Test Engineer

Consult the test-engineer for a test list when the task
has **high uncertainty** — you are unsure what to test or
the testing strategy is non-obvious:

- Design trade-offs to evaluate — multiple valid
  approaches make it unclear which behaviors to assert
- Complex interactions between components — integration
  points where failures are subtle and hard to predict
- Greenfield code with no existing test patterns — no
  existing tests to follow as examples
- The task adds or modifies public API surface — API
  contracts need explicit coverage because callers depend
  on them
- The task changes behavior observable by callers or
  users — behavioral changes need explicit assertions
  even when the code change looks small, because the
  blast radius extends beyond the modified function
- The modified code has no existing test coverage — run
  a quick coverage check or grep for test imports of the
  module. Untested code has unknown invariants that the
  TE can surface before implementation cements them
- The task introduces a new test file — new test files
  establish testing patterns for a module. The TE should
  validate the approach before the pattern propagates to
  future tests

## When to Consult the Security Engineer

Consult the security-engineer for a security assessment
when the task has **high risk** — the blast radius of
getting it wrong includes security implications:

- **Trust boundaries** — code that sits between trusted and
  untrusted contexts (e.g., parsing user-supplied input,
  handling authentication/authorization)
- **Untrusted input** — deserialization, schema validation,
  file path handling, URL parsing from external sources
- **Cryptographic operations** — key management, token
  generation, signature verification, hashing
- **Network-facing code** — HTTP handlers, WebSocket
  endpoints, API routes exposed to clients
- **Secrets handling** — configuration that touches
  credentials, tokens, API keys, connection strings
- **Permission/access control** — code that decides what
  users can see or do
- **Data persistence** — SQL queries, file writes, cache
  operations where injection or corruption is possible

## When to Skip Advisors

Skip both advisors when the task is **low risk and low
uncertainty** — the implementation is straightforward and
the blast radius is contained:

- **Pure functions with existing test patterns** — no I/O,
  no side effects, no external input, and the codebase
  already has tests for similar functions that establish the
  testing approach
- **Internal wiring** — module registration, capability
  flags, handler delegation to existing functions
- **Pattern-following with test coverage** — code
  structurally identical to existing, reviewed code in the
  same codebase, where the pattern's tests also cover the
  new instance (e.g., adding a handler when the handler
  registration has parameterized tests)
- **Test-only changes** — adding or modifying tests without
  changing production code
- **Refactoring** — restructuring code without changing
  behavior or trust boundaries
- **Documentation** — comments, README updates, plan files

"Pattern-following" alone is not sufficient to skip the
test advisor — the pattern must include its test coverage.
Code that follows an existing implementation pattern but
has no corresponding test pattern still has high
uncertainty about what to test.

## Dispatch-Time Assessment (Lead)

This rule loads for both the lead and the developer. The
lead applies it **before sending each task** to the
developer — not just at planning time. Check the task
description against the high-risk and high-uncertainty
indicators above. If any match:

- **Direct the developer to consult the relevant advisor as
  the first step of the task.** State which advisor and why
  — e.g., "consult the security advisor before implementing;
  this task parses untrusted input from HTTP responses."
- **Do not prescribe mitigations yourself.** If you identify
  a security concern, that is a signal to route to the
  security advisor — not a signal that you have sufficient
  expertise to specify the controls. The lead's job is to
  identify the risk category; the advisor's job is to
  specify the response.

This dispatch-time check exists because a production
incident showed that when the lead prescribed mitigations in
a task description ("limit pattern length to ≤1024 chars as
ReDoS guard"), the developer treated security as addressed
and did not consult the security advisor. The advisor would
have identified three additional issues the lead missed.
Surface-level mitigations create a false sense of coverage.

## When in Doubt, Consult

The cost of an unnecessary consultation (a few seconds of
advisor time) is far lower than the cost of missing a
security gap or writing inadequate tests.
