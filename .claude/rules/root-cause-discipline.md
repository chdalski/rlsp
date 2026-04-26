# Fix Causes, Not Symptoms

When a test fails, a build breaks, a metric regresses, or
a deployment fails, the cheapest available fix is often
the one that silences the symptom without explaining what
produced it. Symptom fixes pass code review (the diff is
small and self-consistent), pass the test suite (the
failing test now passes), and ship — leaving the
underlying defect in place to surface again under
different conditions.

## The Failure Mode

Implementors under task-completion pressure optimize for
"the symptom is gone." Two recurring patterns illustrate
the shape:

- **A container build fails after a dependency change.**
  The fix is framed as a platform/architecture setting
  tweak. The actual cause is a stale dependency cache
  built for a different architecture — the platform
  setting was never wrong; the cached state was stale.
- **A container hits its memory limit and crashes.** The
  fix is to raise the memory limit. The actual cause is
  excessive memory consumption that the new limit only
  delays. The next deploy crashes again, slightly later.

The pattern in both: a *configuration* change is
substituted for *diagnosis*. Configuration tweaks are
seductive because they are small, reversible, and
immediately observable — and because they make a failing
signal green without requiring the implementor to
understand the system any better than they did at the
start of the task.

The same shape appears in code: wrapping a previously
unhandled call in a broad `try/except` to "fix" an error,
adding a retry loop to mask a race condition, marking a
flaky test as `skip` or `expected_failure`, or bumping a
timeout to "fix" intermittent failures.

## The Rule

1. **Name the cause before implementing the fix.** State
   in one sentence what produced the failure — not what
   you observed (the symptom), but the underlying
   condition. If you cannot name it, you do not yet have
   a fix; you have a guess. Continue investigating.

2. **Distinguish "the symptom is gone" from "the cause is
   addressed."** A passing test after a configuration
   tweak is evidence that the new value avoided the
   failure condition — not evidence that the original
   condition was wrong. Both are sometimes true; the
   implementor must say which.

3. **Configuration value changes require justification of
   the prior value.** When changing a timeout, memory
   limit, retry count, pool size, buffer size, or similar
   tunable to resolve a failure, the change must explain
   why the *previous* value was wrong — not only why the
   new value works. "We were hitting the limit" justifies
   only that the limit was binding; it does not justify
   that the new limit is correct rather than the next one
   that will bind.

4. **Catch-all handlers, retries, skips, and timeout
   bumps are diagnosis tools, not fixes.** If you add a
   broad `try/except`, a retry loop, a `@skip`, an
   `expect_failure`, or a longer timeout to make a test
   pass, treat the change as a temporary measure that
   must be replaced with a real fix before submitting for
   review — or escalated as a blocker. These constructs
   suppress the symptom while preserving the underlying
   defect, and they normalize once they ship: a future
   reader sees the workaround as the design.

## Handoff Format

When submitting a fix for review, the handoff message
must include a one-line **Cause:** statement — the
underlying condition the fix addresses, not the symptom
it was discovered through. Examples:

- *Cause:* `node_modules` was rebuilt locally on arm64
  and copied into an amd64 image; the COPY in the
  Dockerfile bypassed the arch-aware install step.
- *Cause:* the request handler held the parsed body in
  memory for the full response lifetime; under
  concurrent load, peak resident memory exceeded the
  container limit.

A handoff with no Cause line, or one that restates the
symptom ("the test was failing"), signals that diagnosis
is incomplete.

## Reviewer Backstop

When reviewing a diff that consists primarily of one of
the patterns below, verify the handoff's Cause line names
the underlying condition — not just the change:

- A configuration value change (timeout, limit, retry,
  pool, buffer)
- A new broad exception handler around previously
  unhandled code
- A new retry loop, sleep, or backoff added to existing
  control flow
- A test marked as skipped, expected-to-fail, or
  conditionally-disabled
- A change that disables, weakens, or short-circuits an
  existing assertion

If the handoff has no Cause line, or the Cause restates
the symptom rather than identifying its source, reject
and ask for it. The change may still be the right
answer — but the reasoning has to be explicit, not
assumed from the fact that the symptom went away.

## Why This Matters

Symptom fixes are systemically harder to catch than other
defects because every downstream signal looks correct:
the test passes, the build is green, the deploy succeeds,
the diff is small and obviously related to the failure.
There is nothing for the reviewer to flag from the diff
alone. The defect resurfaces later — under load, on a
different platform, after a dependency upgrade — at which
point the original context is lost and the next
implementor inherits a system whose configuration encodes
prior workarounds rather than intentional design.

Implementor optimization pressure makes this failure mode
particularly likely: a fast local fix that closes a task
is more rewarded than a slower investigation that
reschedules it. This rule shifts the burden — naming the
cause is a precondition for the fix, not a documentation
nicety after it.
