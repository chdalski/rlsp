---
name: Scratch measurement tools are not deliverables
description: When dispatching a task that requires empirical measurement, explicitly state that scratch tooling (throwaway tests, measurement binaries, debug scaffolds) is not a committable artifact
type: feedback
originSessionId: 4cbb91b1-6f2e-4945-9836-1f67b91dfb9c
---
When a task requires the developer to measure behavior empirically — running the formatter, probing an API, checking wrap boundaries, etc. — the developer often creates a scratch Rust/TS file to do the measurement. That scratch file is a **tool**, not a **deliverable**. The user flagged this during Task 5 of a formatter-fixture plan: the developer had created `rlsp-yaml/tests/measure_tabs_interaction.rs` and left it in the working tree; without intervention it could have ended up in the commit.

**Why:** Task dispatch messages already list the exact files involved (e.g. "Files involved: `fixtures/foo.md` (new)"). But when a task includes an investigative step, the developer creates helper files that are not in that list. Without an explicit reminder, those files survive until the reviewer catches them (or worse, until after commit). The reviewer's file-list cross-check is a safety net, not the primary line of defense.

**How to apply:** When a task dispatch includes any investigative/measurement step, add an explicit bullet like: "Any scratch code written to perform the measurement is not a deliverable. Delete it before submitting. Only the files listed in 'Files involved' should appear in the final `git status`." This turns the convention into a checkable requirement instead of relying on the developer's judgment mid-task.
