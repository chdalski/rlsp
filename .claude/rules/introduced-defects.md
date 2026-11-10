# Introduced Defects

A defect the current change causes is fixed in the same
task, whatever its severity. Severity decides what may stay
open only for defects that were there before the change and
for findings that are no defect. Rated Low, a bug in the
code a task has just written passes the review, and the
task's own commit files it in `.ai/backlog.md`: in a prior
project a reviewer found that the second click of a double
click on a new button could open the next board one level
too deep, and the bug went into the backlog with the
button. The plan that later fixed ten such entries added
five new ones from its own fixes; one entry even spelled
out its one-line fix.

## Terms

- **Defect** — wrong behavior: a wrong result, lost or
  changed data, a crash or hang, input or state handled
  differently than the code promises. A regression is a
  defect too: something that held before the change no
  longer holds, including a runtime or memory cost that a
  measurement shows and the plan does not accept. A finding
  about structure, naming, tests, documentation, or
  hardening the code does not need to behave correctly is
  no defect.
- **Introduced** — the change causes the defect: it lies in
  code the change adds or alters, or it is a regression.
- **Pre-existing** — the code before the task shows the same
  defect. Check that before you say so: read the code at the
  task's baseline (`git show <baseline>:<path>`; without
  commits in the task, the baseline is `HEAD`) or run the
  case there, and name that command and its result with the
  finding. Until you have, the defect counts as introduced.
  A defect in lines the change adds is introduced, even when
  it looks like a known one. A prior reviewer labeled a
  defect "pre-existing, not verified at baseline" and
  approved; the function it lay in did not exist at the
  baseline.

## The Rule

1. **Every finding on a task's change names its origin**
   next to its severity: `introduced`, `pre-existing`, or
   `no defect`. Findings on plans and designs carry none.
2. **An introduced defect blocks** the review approval and
   any sign-off that reports it, at every severity, until it
   is fixed or the user accepted it.
3. **Its fix is part of the task.** When the fix needs work
   beyond the task or a cost the user weighs — a slower
   path, a behavior change elsewhere — the implementor
   reports it to the requester, and the lead puts it to the
   user: fix it, in this task or through a plan revision, or
   accept the defect. The implementor names an accepted
   defect in the next review request.
4. **The backlog takes an introduced defect only after the
   user accepted it**, and its entry says so. Pre-existing
   defects and findings that are no defect go there as
   before.

A lower severity, a note in the approval, a backlog entry,
or a later bug-fix plan does not replace the fix: each
leaves the defect in the commit the gate approved.
