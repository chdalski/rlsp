# Procedural Fidelity

When you encounter a numbered procedure — in a skill, a
checklist, your own instructions, or a task description —
execute every step in order and do not skip steps.

## The Failure Mode

Agents under optimization pressure pattern-match against
procedures instead of executing them sequentially. The
typical failure: an early step produces observable state
(a directory exists, a config key is present, tests
passed last time), and the agent treats that observation
as proof that later steps are unnecessary. This is a
**sufficiency fallacy** — a necessary condition (step 1
succeeded) is mistaken for a sufficient condition
(everything is current).

This has caused real production failures: a lead
short-circuited a three-step skill after step 1 ("config
found"), skipping the unconditional format-guide overwrite
in step 2, and produced four plans against a stale
template.

## The Rule

1. **Execute every numbered step.** If a step says "always
   do X," do X — even if the result appears to already
   exist. The step exists because prior state can be stale
   in ways that are not visible without executing the step.

2. **Do not infer step results.** Reading that a file
   exists is not the same as writing it. Seeing that tests
   passed yesterday is not the same as running them now.
   Each step produces its own evidence — execute it and
   observe the actual result.

3. **Report what you did, not what you assumed.** When
   completing a procedure, your summary should reference
   the output of each step. If you cannot cite a step's
   output, you skipped it.

## Why This Matters

Procedures encode sequencing decisions made by the
instruction author. The author knew which steps are
unconditional and which are conditional — that information
is in the step text, not in the surrounding state. An
agent that second-guesses this sequencing substitutes its
own incomplete model for the author's complete one.
