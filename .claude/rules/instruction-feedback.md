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

Auto memory is no substitute: it is not reviewed with the
change, and the next agent to run a command reads it from
`CLAUDE.md`.

## Blueprint Files

Do not edit files under `.claude/` — `.claude/CLAUDE.md`
included — to fix an instruction there unless your plan
or another rule calls for that change. They are copied
from a blueprint the user maintains and steer every agent
in every session: an edit made in passing changes how all
agents work without the user deciding it, and diverges
from the blueprint unnoticed. If a blueprint instruction
is wrong for this project or cannot be followed as
written, include the file, the instruction, and what
happened in your next message to the requester. The lead
passes these reports to the user at the next user
checkpoint, or right away when the workflow has none.

When the user corrects how the workflow itself runs — how
plans are reviewed, how teammates are shut down, what to
check before a commit — the lead proposes the change to
the `.claude/` file that governs it and asks whether the
blueprint should receive it too. A lesson kept only in
memory holds only in sessions that recall it, and no
review ever checks it.
