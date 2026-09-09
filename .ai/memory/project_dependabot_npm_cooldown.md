---
name: Dependabot npm runs fail on its own 3-day cooldown
description: npm_and_yarn Dependabot job fails intermittently with ERR_PNPM_NO_MATURE_MATCHING_VERSION; cause is Dependabot's injected minimumReleaseAge=4320, not repo config
type: project
---

Dependabot invokes pnpm with its own supply-chain cooldown injected on the
command line:

```
pnpm update <dep> --lockfile-only --no-save -r --config.minimumReleaseAge=4320
```

4320 minutes = 72 hours. **This is not repo configuration** — there is no
`minimumReleaseAge` anywhere in the repository, no `.npmrc`, and the
`pnpm` block in the VS Code extension's `package.json` holds only `overrides`
and `auditConfig`. Do not go looking for it in the project; you will not find
it, and adding one is not what makes this happen.

When any package in the resolution tree was published less than 72 h before
the run, pnpm aborts:

```
ERR_PNPM_NO_MATURE_MATCHING_VERSION  Version X (released N hours ago) of
<pkg> does not meet the minimumReleaseAge constraint
```

pnpm exits 1 → `Dependabot::SharedHelpers::HelperSubprocessFailed` → the whole
job is recorded as `record_update_job_unknown_error`, attributed to whichever
dependency it happened to be processing (misleading — that dependency is not
the problem).

**It self-heals.** The blocking package matures 72 h after its publish date and
the next scheduled run passes. Observed 2026-09-08: `ast-v8-to-istanbul@1.0.6`
(published 2026-09-07, a transitive dep of the installed
`@vitest/coverage-v8@5.0.0`) blocked the run, while that same version was
already pinned in the committed `pnpm-lock.yaml` — the project's own installs
were unaffected, only Dependabot's fresh re-resolution. Group runs on 08-20,
08-27 and 09-03 succeeded, so collisions are occasional, not constant.

Treat a red `npm_and_yarn` Dependabot run as noise unless it repeats past the
72 h window. `minimumReleaseAgeExclude` (https://pnpm.io/settings#minimumreleaseageexclude)
would silence it per-package but weakens the supply-chain guard and is
whack-a-mole; a `cooldown:` key in `.github/dependabot.yml` would not help,
since it governs which updates Dependabot proposes, not what pnpm resolves.

Distinct from the `for <dep>` **security** runs (e.g. `for fast-uri`,
`for @humanfs/node`), which failed with `security_update_not_possible` and are
resolved — all alerts are `fixed`, the lockfile carries `fast-uri@3.1.7`, and
zero alerts are open as of 2026-09-09. See [[project_followup_plans]].
