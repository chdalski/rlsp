# Testing Principles

## What Makes a Good Test

### Independence

- Each test must run in isolation
- No test should depend on another test's state or
  execution order
- Tests should set up their own preconditions and clean up
  after themselves

### Determinism

- Same inputs must always produce same results
- No reliance on external state, timing, or randomness
- Flaky tests erode confidence and must be fixed immediately

### Clarity

- Test names describe the behavior being verified, not the
  implementation
- One assertion per test when possible
- Use the Arrange-Act-Assert pattern

```pseudocode
test order_id_rejects_negative_value:
    // Arrange
    input = -1

    // Act
    result = OrderId.new(input)

    // Assert
    assert result is Error(NegativeIdError)
```

### Focus

- Test behavior, not implementation details
- Tests should survive refactoring if behavior is unchanged
- Avoid testing private internals — test through the public
  interface

## Testing Pyramid

Tests serve different purposes at different levels of the
system. Use the right type of test for what you're verifying.

### Unit Tests

- Test a single function, method, or type in isolation
- No external dependencies (no network, filesystem, database)
- Fast — the entire unit test suite should run in seconds
- Co-locate with the code they test when the language
  supports it (e.g., inline test modules)
- This is the base of the pyramid — most tests should be
  unit tests

### Integration Tests

- Test how components work together across module or
  layer boundaries
- May use real infrastructure (databases, file systems)
  via test containers or fixtures
- Slower than unit tests — run selectively or mark for
  separate execution
- Test through ports and adapters, not through the full
  application stack
- Mock only what is impractical to run (external APIs,
  third-party services)

### End-to-End Tests

- Test the full application from external input to
  observable output
- Verify that the system works as a user would experience
  it
- Slowest and most brittle — keep the count low
- Focus on critical user journeys, not exhaustive coverage
- Use for smoke testing and acceptance criteria, not for
  finding bugs

### Choosing the Right Level

- **Business logic, value types, pure functions** →
  unit test
- **Cross-module interactions, database queries, HTTP
  handler routing** → integration test
- **Critical user workflows, deployment verification** →
  end-to-end test
- When in doubt, push the test down to the lowest level
  that can verify the behavior. A unit test that covers
  the logic is better than an integration test that
  exercises the same path with more overhead.

### Mocking Strategy

- Mock at system boundaries (ports, external services),
  not within the domain
- If the architecture uses ports (traits/interfaces),
  implement test doubles directly — prefer hand-written
  mocks over mocking libraries when practical
- Real collaborators within the same layer are preferable
  to mocks — only mock what you must

## Test Design

### Coverage Strategy

- Start with happy paths (expected usage)
- Add edge cases (boundaries, empty inputs, maximums)
- Add error conditions (invalid input, failures)
- Order from simple to complex to guide incremental
  development

### Naming

- Names should read as behavior specifications
- Pattern: `should_<expected_behavior>_when_<condition>`
- Bad: `test_calculate`, `test1`, `testError`
- Good: `should_return_zero_for_empty_input`,
  `should_reject_negative_values`

## Anti-Patterns

### Excessive Mocking

- Mocks that replicate implementation details make tests
  brittle
- See Mocking Strategy above for where to mock

### Implementation Coupling

- Tests that break when internals change but behavior stays
  the same
- Testing private methods directly
- Asserting on internal state instead of observable output

### Test Interdependence

- Shared mutable state between tests
- Tests that must run in a specific order
- Setup in one test that another test relies on

### Ignoring Warnings

- Compiler and type-checker warnings in test code matter
- Suppressing warnings hides real problems
- Treat test code with the same rigor as production code
