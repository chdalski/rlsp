# Acceptance Criteria Verification

When a task or plan includes a quantitative target — a
pass rate, coverage percentage, latency threshold,
conformance score, or any other measurable criterion —
that target is a concrete acceptance criterion, not an
aspiration. "Tests pass" is not equivalent to "target met"
when the target is a specific metric.

## The Failure Mode

A plan specifies "100% conformance." The implementor builds
infrastructure and fixes some failures. The project's own
tests pass. A reviewer sees clean code, passing tests, and
self-consistent work — and approves. But the conformance
suite was never run against the target, so 81% conformance
ships as "delivered" against a 100% target.

This happens because "all tests pass" is silently conflated
with "the acceptance criterion is met." These are distinct
assertions — the project's test suite validates internal
correctness, while a quantitative target measures a
specific external property that passing tests alone do not
cover.

## The Rule

1. **Measure independently.** When implementing a task
   with a quantitative target, run the measurement that
   corresponds to the target and report the actual result.
   Do not infer the result from other signals — a clean
   build and passing tests are necessary but not sufficient
   when the target is a specific metric.

2. **Include the measurement in handoffs.** When sending
   completed work for review, state the target, the
   measured result, and whether the target was met.
   Example: "Target: 100% conformance. Result: 289/308
   valid (93.8%), 94/94 invalid (100%). Target not met —
   19 valid-YAML failures remain."

3. **Verify the measurement at review.** When reviewing
   work that has a quantitative target, check that the
   handoff includes the measured result. If it does not,
   reject and ask for the measurement. If the result falls
   short of the target, the task is incomplete regardless
   of code quality — reject for incomplete scope.

4. **Design tests against the target.** When designing
   test specifications for work with a quantitative
   target, include the target metric as an explicit
   verification step — what command to run, what output
   to compare, and what threshold constitutes success.
   This makes the target measurable by the implementor
   without guesswork.

## Why This Matters

Quantitative targets are the hardest acceptance criteria
to verify because they require a specific measurement
action. Unlike code quality or structural completeness —
which are evaluated by reading the diff — a metric must
be actively measured. Without explicit measurement
discipline, the pipeline silently degrades: each agent
does its job correctly (clean code, passing tests, good
structure), but the aggregate does not meet the stated
goal. The gap is invisible until someone runs the
measurement independently.
