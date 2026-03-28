**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-28

## Goal

Fix two findings from the `/project-sanity` audit of the
release workflow: add missing permissions to the
`filter-binaries` job and replace the `softprops/action-gh-release`
action (which uses deprecated Node.js 20) with the `gh` CLI.

## Context

**Finding 1 (high):** `release-plz.yml` `filter-binaries`
job (line 48) has no `permissions` block — CodeQL alert #4.
This job only runs a shell script with `jq` and needs no
token permissions, so `permissions: {}` is appropriate.

**Finding 2 (medium):** `softprops/action-gh-release@v2`
(lines 122, 129) uses Node.js 20, deprecated as of 2026.
No v3 exists. User chose to replace it with `gh release
upload` CLI — zero third-party dependency, pre-installed
on runners, no Node.js version concern.

**Key file:** `.github/workflows/release-plz.yml`

**Current upload steps (lines 120-132):**
```yaml
- name: Upload to GitHub Release (Linux/macOS)
  if: matrix.target.os != 'windows-latest'
  uses: softprops/action-gh-release@v2
  with:
    tag_name: ${{ matrix.crate.tag }}
    files: ${{ matrix.crate.crate }}-${{ matrix.target.target }}.tar.gz

- name: Upload to GitHub Release (Windows)
  if: matrix.target.os == 'windows-latest'
  uses: softprops/action-gh-release@v2
  with:
    tag_name: ${{ matrix.crate.tag }}
    files: ${{ matrix.crate.crate }}-${{ matrix.target.target }}.zip
```

**Replacement with `gh`:** The `gh release upload` command
can upload assets to an existing release by tag. The
`build-binaries` job already has `permissions: contents: write`
and `GITHUB_TOKEN` is available by default. The `gh` command
works on all runner OSes (Linux, macOS, Windows).

```
gh release upload <tag> <file> --clobber
```

The `--clobber` flag replaces existing assets with the same
name, which is useful if a job is retried.

## Steps

- [x] Audit workflow with /project-sanity
- [x] User approved fixes
- [ ] Fix both findings in release-plz.yml

## Tasks

### Task 1: Fix release-plz.yml sanity findings

Apply both fixes to `.github/workflows/release-plz.yml`:

1. Add `permissions: {}` to the `filter-binaries` job
   (after line 51, before `outputs:`). This job runs only
   a shell script — it needs no GITHUB_TOKEN permissions.

2. Replace the two `softprops/action-gh-release@v2` steps
   (lines 120-132) with `gh release upload` CLI steps:

   Linux/macOS:
   ```yaml
   - name: Upload to GitHub Release (Linux/macOS)
     if: matrix.target.os != 'windows-latest'
     run: gh release upload ${{ matrix.crate.tag }} ${{ matrix.crate.crate }}-${{ matrix.target.target }}.tar.gz --clobber
     env:
       GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
   ```

   Windows:
   ```yaml
   - name: Upload to GitHub Release (Windows)
     if: matrix.target.os == 'windows-latest'
     run: gh release upload ${{ matrix.crate.tag }} ${{ matrix.crate.crate }}-${{ matrix.target.target }}.zip --clobber
     env:
       GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
   ```

## Decisions

- **`permissions: {}`** for filter-binaries — empty rather
  than `contents: read` because the job genuinely needs no
  token access; it only runs `jq` on a JSON string passed
  via outputs
- **`gh` CLI over action** — eliminates third-party
  dependency and Node.js deprecation concern entirely.
  `--clobber` handles retries gracefully.
- **`GH_TOKEN` env var** — `gh` uses this automatically;
  explicitly setting it from `secrets.GITHUB_TOKEN` makes
  the authentication visible in the workflow
