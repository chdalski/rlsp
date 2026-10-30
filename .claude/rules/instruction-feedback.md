# Instruction Feedback

The project's `CLAUDE.md` files are the first thing every
session trusts: the Build and Test commands, the
Conventions, the References, the paths. When your work
shows that an entry is wrong, or that a fact you needed
is missing, what you found is knowledge the next session
needs. Working around it and moving on loses it with the
session, and the next session pays to rediscover it.

## The Rule

1. **Verify before you report.** Name the file, the entry
   as written, what actually works, and the evidence —
   the command you ran and its result. Label a correction
   you have not verified as unverified.

2. **Correct wrong entries in the change set.** If your
   task lets you edit project files, fix the entry and
   name the fix in your completion report or review
   request, so the review covers it before the commit.
   Otherwise, include the entry and its correction in
   your next message to the requester — for the lead,
   the user.

3. **Add missing facts only where the file invites it.**
   Add a one-line entry to a Conventions or References
   section only if the section carries an
   `<!-- Agents: ... -->` comment. Read the file to see
   it — Claude Code strips block-level HTML comments from
   auto-loaded `CLAUDE.md` content. Without the comment,
   pass the fact on the same way: the user wrote that
   file and decides what goes in it. The bar is "would a
   future agent make a mistake without this?" — a fact
   that only saves time stays out.

4. **Put each fact where it survives.** `/project-init`
   regenerates the Build and Test section from manifests.
   When a correction adds a detail the manifests do not
   declare — a required flag, environment variable, or
   setup step — correct the command in place and also
   record the detail as a Conventions line, or the next
   regeneration erases it.

## Files Under `.claude/`

Do not edit files under `.claude/` — `.claude/CLAUDE.md`
included — to fix an instruction there unless your plan
or another rule calls for that change. They steer every
agent in every session: an edit made in passing changes how
all agents work without the user deciding it. If an
instruction there is wrong for this project or cannot be
followed as written, include the file, the instruction, and
what happened in your next message to the requester. The
lead passes these reports to the user at the next user
checkpoint, or right away when the workflow has none.

When the user corrects how the workflow itself runs — how
plans are reviewed, how teammates are shut down, what to
check before a commit — the lead proposes the change to
the `.claude/` file that governs it. A lesson kept only in
the session ends with it. Write the edit like the
neighbouring steps and change every step that encodes the
old sequence, not only the one the correction names: a
partial edit leaves the file contradicting itself.

## Where Knowledge Goes

Auto memory is switched off in `.claude/settings.json`, and
nothing replaces it as a scratch pad. Knowledge worth keeping
has one place each, where a review or a later session finds
it:

| What | Where |
|---|---|
| How the team works | A `.claude/` change the lead proposes |
| A project fact or pitfall | `CLAUDE.md` (root or subdirectory), docs, or a code comment, per the rules above |
| A hint for a later task of a running plan | The plan's Task Notes |
| The state of running work | Plan checkboxes and Git, nothing else |
| An open finding or cleanup wish | `.ai/backlog.md` |
| Everything else | Stays in the session |

The backlog holds one line per item: what, where, and where
it came from. The lead owns the file: it adds entries without
asking, names them in the completion report, and removes one
when it becomes a plan or the user drops it. It is no status
log and no home for facts. A missed plan target goes there
only after the user has lowered the target
(`no-silent-target-weakening.md`).
