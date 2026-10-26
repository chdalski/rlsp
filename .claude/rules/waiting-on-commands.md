# Waiting on Commands

When you need the result of a long-running command — a
build, a test suite, a pipeline — wait for the event that
reports its end, not for a loop you write to watch it. A
hand-written watch loop decides on its own when the work is
over, and it gets that wrong more easily than it looks.

## The Failure Mode

Watch loops in a prior project ran long after the work they
watched had ended, or could never end at all:

- A loop waiting for a CI pipeline exited only on the
  success state of one expected commit and discarded its
  probe's errors with `2>/dev/null`. It kept polling for
  twenty minutes after the pipeline had finished, until the
  lead stopped it by hand.
- `pgrep -f cargo`, meant to wait until no build runs,
  matched the shell running the check — its command line
  contains "cargo" — so "no cargo process left" could never
  come true.
- Loops waiting for a marker such as `Summary` in a test
  log hung until their timeout when the run failed before
  printing it.

Rewriting a blocked `sleep` as
`bash -c 'until …; do sleep 15; done'` is the same loop
with the same flaws.

## The Rule

1. **For a command you start, rely on its completion.** Run
   it in the foreground with a `timeout` that covers it, or
   with `run_in_background` — Claude Code notifies you when
   it exits, even after your turn has ended. Do not write a
   loop that watches your own command, its output file, or
   its processes.

   **Teammates run in the foreground.** If you are a
   teammate (spawned with a `name`), never use
   `run_in_background` for a command whose result you need
   before you can reply — a build, a test suite. Run it in
   the foreground with a `timeout` that covers it (up to
   600000 ms); split a longer run into foreground steps.
   The completion notice does not reach an idle teammate:
   a reviewer here started its clean build and tests in
   the background three times, ended its turn to wait, and
   stayed idle after the command had exited until the lead
   messaged it — each approval came 5–8 minutes late.

2. **Watch only what you did not start** — a CI pipeline, a
   deployment, a container becoming healthy. Such a loop
   must:
   - exit on every terminal state, failure included, and
     print which one it saw;
   - exit with the error when its probe fails — never
     discard probe errors with `2>/dev/null` and retry;
   - stop at a deadline that fits the expected duration,
     e.g. `timeout 900 bash -c 'until …; do …; done'`.

3. **Never probe with a pattern your own command line
   contains.** `pgrep -f <pattern>` and `ps | grep
   <pattern>` match the shell that runs the check. Wait on
   a PID you recorded, or match the exact process name with
   `pgrep -x`.

## Why This Matters

A watch loop that misses its end condition looks exactly
like work still in progress: the agent waits, and so does
everyone waiting on that agent. A completion notification
comes from the command itself, so it cannot miss the end.
