# Workflow Format

Every workflow file in this directory defines a reusable
execution pattern. The lead reads these files to present
workflow options to the user. Keeping workflows as
separate files means adding a new workflow requires no
changes to CLAUDE.md or other configuration — just add
a file.

## Required Sections

### Name

A short, descriptive name. Use as the markdown heading —
the lead uses this name when presenting options to the
user.

### When to Use

Conditions where this workflow is the right choice:
- Task characteristics (size, complexity, risk)
- Team preferences (autonomy vs. control)
- Project context (new codebase, legacy, greenfield)

Be specific — the lead uses this section to match
workflows to tasks and explain trade-offs. Vague
conditions like "for complex tasks" don't help the lead
reason about which workflow fits.

### Agents

Which agents this workflow requires. Reference agent
names from `.claude/agents/` — this ensures the lead
creates the right agents with the right tool sets and
instructions. For each agent, note its role in this
specific workflow, since the same agent may serve
different purposes in different workflows.

```markdown
- **Developer** — implements code and tests
- **Test Engineer** (advisory) — designs test spec,
  verifies coverage
```

### Flow

Step-by-step execution order. Number each step. Mark
handoff points between agents and user checkpoints
clearly — numbered steps prevent ambiguity about
ordering, and explicit checkpoints ensure the user
stays in control where it matters.

```markdown
1. Lead sends clarified task to Test Engineer
2. Test Engineer produces test spec
3. **User checkpoint** — review and approve test spec
4. Lead sends approved spec to Developer
5. Developer writes tests, then implements
6. Test Engineer verifies tests match spec
7. **User checkpoint** — review implementation
```

Use **User checkpoint** to mark where the user is
consulted. Use agent names to show who acts at each step.

### Completion Criteria

How to know the workflow is done. Be explicit — without
clear criteria, the lead has to guess when to stop,
which either cuts work short or wastes tokens on
unnecessary iterations.

```markdown
- All tests pass
- Test Engineer has given sign-off
- User has approved the final result
```

## Shared Agents

All agents (Architect, Developer, Test Engineer, Security
Engineer, Reviewer) are general-purpose building blocks —
no agent runs automatically. Each workflow declares which
agents it needs in its own Agents table. The Reviewer
appears in every workflow as an independent quality gate
(including CLAUDE.md drift detection) and is responsible
for committing approved work — it has full context from
the review to write an accurate commit message. The lead
creates one team via `TeamCreate` with all listed agents
so they can communicate via `SendMessage`. This applies
to all workflows including Direct-Review, where a one-agent
team allows the Reviewer to receive the commit signal after
the user checkpoint.

## Conventions

- "When to Use" sections describe the workflow from the
  user's perspective — the user selects the workflow, the
  lead presents options. Describe what gates the workflow
  provides, what autonomy level it offers, what the user
  sees and approves. Do not use magnitude qualifiers
  (`trivial`, `simple`, `complex`, `large`, etc.) or
  agent-gating phrases that imply the agent judges task
  size — that judgment belongs to the user. If the
  workflow is structurally wrong for a task category
  (not wrong because of size), state it concretely:
  "Not appropriate for non-code tasks."

- One workflow per file — mixing workflows in one file
  makes it harder for the lead to present individual
  options to the user.
- `develop-review-supervised.md` and
  `develop-review-autonomous.md` share an identical
  dev-team flow (steps 1–14) — only the Commit section
  differs. When editing either file's dev-team flow,
  apply the same change to the other.
- File names should be descriptive:
  `tdd-user-in-the-loop.md`, not `workflow-001.md` —
  the lead may scan file names to quickly identify
  candidate workflows.
- Keep workflows focused — a workflow that tries to
  cover every scenario is too broad to reason about,
  and the lead cannot clearly explain its trade-offs.
- Workflows reference agents but do not define them —
  agent definitions live in `.claude/agents/`. This
  separation means agent capabilities and workflow
  patterns evolve independently.
- The same agent can appear in multiple workflows with
  different roles — agents are general-purpose building
  blocks, workflows are specific compositions.
