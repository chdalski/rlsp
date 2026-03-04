# Simplicity Principles

These principles apply to everything agents produce —
code, documentation, configuration, plans, commit messages,
and architecture decisions. They are not code-specific.

## Reveals Intent

Everything you produce should clearly express what it does
and why — unclear artifacts waste the reader's time and
invite misinterpretation:

- Use meaningful names for variables, functions, types,
  files, and sections
- Structure content to be self-documenting
- Prefer explicit over clever
- Comments and annotations explain "why", not "what"

## KISS — Keep It Simple

Choose the simplest solution that works — complexity is a
cost that compounds over time through maintenance burden,
onboarding friction, and bug surface area:

- Avoid unnecessary complexity in data structures,
  algorithms, architecture, and configuration
- If a solution is hard to explain, it's probably too
  complex
- Simple solutions are easier to test, debug, and maintain

## YAGNI — You Aren't Gonna Need It

Don't build for hypothetical future requirements — premature
generalization adds complexity that may never pay off, while
the cost of carrying it is immediate:

- Don't implement functionality until it is needed
- Don't design for speculative use cases
- Remove unused code, config, and dead abstractions
- Premature generalization is a form of over-engineering

## Fewest Elements

Minimize the number of types, functions, files, config
options, and moving parts — every element has a maintenance
cost, and unnecessary abstractions obscure intent:

- Remove unnecessary abstractions
- Only add complexity when it serves a clear purpose
- A simple function beats an interface with one
  implementation
- Three similar lines of code are better than a premature
  abstraction

## When Principles Conflict

These principles sometimes pull in opposite directions.
When they do, use this priority:

1. **Correctness** — it must work (tests pass, docs are
   accurate)
2. **Clarity** — it must be understandable (Reveals Intent)
3. **No duplication** — knowledge should live in one place,
   but not at the cost of clarity
4. **Fewest elements** — minimize parts, but not at the
   cost of clarity or correctness

This is the same priority order as Kent Beck's Four Rules
of Simple Design (in `code-principles.md`), generalized
beyond code. Kent Beck's rules apply specifically to source
code; this ordering applies to everything.

When in doubt, choose the option that is easier to change
later. Simple code is easy to make complex; complex code is
hard to make simple.
