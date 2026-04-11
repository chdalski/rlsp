# No Silent Target Weakening

When a quantitative acceptance target exists —
a conformance pass rate, a coverage percentage, a latency
threshold, or any other measurable criterion — and the
measured result falls short, there are exactly two options:

1. **Fix to meet the target.** Do the work required to
   close the gap between the measured result and the
   stated target. This is the default.

2. **Ask the user to explicitly lower the target.** If
   the gap is genuinely too large to close within the
   current scope, present the measured result, the target,
   the specific shortfall, and ask the user whether they
   want to lower the target or extend the scope. The user
   decides — not the lead, not the developer, not the
   reviewer.

There is no third option. Specifically, these are all
disguised versions of option (2) that bypass user consent:

- Repackaging the gap as "follow-up work" or a "future
  plan item"
- Moving shortfall items to a post-milestone cleanup list
- Reframing "81 failures" as "deferred to post-migration"
- Accepting a partial result as "good enough for the use
  case" without user approval
- Documenting the gap in memory as a todo instead of
  fixing it now

Each of these silently weakens the target the user
approved. The user approved a plan with a specific number.
Delivering less than that number without their explicit
sign-off is incomplete delivery — regardless of how the
shortfall is packaged.

## The Test

Before accepting any result that falls short of a
quantitative target, ask: "Did the user explicitly agree
to lower this target?" If the answer is no, the work is
not done.
