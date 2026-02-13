# Test List Workflow

This practice defines the test-list-driven development
workflow. The Test Engineer designs what to test, the
Developer writes and implements all code.

For test design principles (structure, naming,
anti-patterns), see `knowledge/base/testing.md`.

## Why Test Lists

A test list separates test *design* from test
*implementation*. The Test Engineer focuses on coverage
— what scenarios, edge cases, and security cases matter.
The Developer focuses on execution — writing tests and
source code without file-ownership handoffs.

This avoids the stop-start cycles of split ownership
(where the Developer waits for the Test Engineer to
finish writing test files) while preserving independent
test design review.

## Step 1: Test Engineer Produces the Test List

Before any code is written, the Test Engineer analyzes
the task and produces a structured test list:

- **Happy paths** — expected behavior with valid inputs
- **Edge cases** — boundary values, empty inputs, limits
- **Error conditions** — invalid inputs, failures,
  timeouts
- **Security scenarios** — identified by the Security
  Engineer (input validation, auth, error leakage)

Each test case includes:

- A descriptive name (what behavior is expected)
- The scenario (inputs and state)
- The expected outcome (what to assert)

Order from simple to complex — this guides the
Developer's implementation sequence.

```pseudocode
test list:
    Unit Tests:
        should_return_zero_for_empty_input
            scenario: pass empty collection
            expect: returns 0

        should_return_number_for_single_input
            scenario: pass collection with one element
            expect: returns that element

        should_return_sum_for_two_numbers
            scenario: pass [3, 5]
            expect: returns 8

        should_handle_negative_numbers
            scenario: pass [-1, 2, -3]
            expect: returns -2

        should_reject_non_numeric_input
            scenario: pass collection with non-numeric
            expect: returns error

    Integration Tests:
        should_calculate_via_api_endpoint
            scenario: POST /calculate with valid body
            expect: 200 with correct result
```

## Step 2: Developer Writes All Tests

The Developer writes all tests from the test list in
a single batch:

- If integration tests are included, spike one first
  to validate the test harness. Fix any framework-level
  issues before writing the rest.
- Write all unit and integration tests together.
- Do not split into phases.

## Step 3: Test Engineer Verifies Tests

Before the Developer starts implementing source code,
the Test Engineer reviews the written tests against the
original specification:

- Every test case from the list must be present
- Test names, scenarios, and assertions must match
- Missing or incorrect tests are sent back for fixes

The Test Engineer sends "tests verified" when satisfied.
This checkpoint catches gaps between the specification
and the actual tests before implementation begins —
fixing them later costs significantly more.

## Step 4: Developer Implements

The Developer implements source code to make all tests
pass. During implementation:

- Do not skip, weaken, or remove tests
- If a test seems wrong, discuss with the Test Engineer
  rather than changing it unilaterally

## Step 5: Post-Implementation Sign-Offs

After implementation, two independent sign-offs are
required before the dev-team reports completion:

1. **Test sign-off** (Test Engineer) — verifies tests
   were not altered during implementation and coverage
   still matches the original specification
2. **Security sign-off** (Security Engineer) — verifies
   security concerns were addressed in the code

Both sign-offs exist because the Developer owns all
code. Without independent verification, the Developer
could (intentionally or not) weaken tests or skip
security considerations during the pressure of
implementation.
