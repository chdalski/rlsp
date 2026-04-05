# Safe Git Operations

Before running any destructive git command on the working
tree, verify that uncommitted changes are protected.
Destructive operations permanently discard uncommitted
changes — hours of work that exists only in the working
tree is irrecoverable after a `git checkout --`,
`git reset --hard`, or `git clean`.

## The Rule

Before running `git checkout -- <file>`, `git reset
--hard`, `git clean -f`, or `git restore <file>`:

1. Run `git status` to check for uncommitted changes.
2. If the working tree is dirty, commit or stash first:
   `git stash push -m "before <operation>"` or
   `git add <files> && git commit -m "wip: <desc>"`.
3. Only then run the destructive command.

## Why

A production incident lost ~2 hours of developer work
when `git checkout -- <file>` was used during debugging
to test whether specific files caused a regression. With
all modified files checked out, the entire working tree
was reset and no stash or WIP commit existed to recover
from.

## Scope

This applies to any agent with Bash access. The WIP
commit protocol in the developer's instructions provides
the primary protection against work loss — this rule is
the safety net for cases where WIP commits are not
current or stashing is more appropriate than committing.
