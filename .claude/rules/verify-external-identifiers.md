# Verify External Identifiers

An identifier that names something outside the repository
— a container image tag, a package name, a CI action
reference, a crate or package version, an API endpoint or
a configuration key taken from a vendor's documentation, a
URL — is a claim that this exact string exists out there.
Nothing in the repository can check the claim: the build,
the tests and an offline link check all accept any string.
The first thing that checks it is the next person's
rebuild, install or click. The rule covers identifiers a
tool will consume, and URLs when they are added to a
document; a name in prose that no tool reads, such as a
company or a book title, is not in scope.

## The Failure Mode

An agent sees the identifier in a listing that is
filtered, truncated or paraphrased, infers the exact form,
and writes the inferred string. A prior session wrote the
image tag `ubuntu-26.04` into a Dockerfile after seeing
tags that contained "26.04" in a registry listing cut to
twelve entries. The real tag is `ubuntu26.04`; the user's
container rebuild failed, and the user had to fix the tag.
The same session confirmed a package name against the
distribution's package index for the exact name and
release before writing it, and it worked.

## The Rule

1. **Before writing an external identifier into a
   committed file, confirm that the exact string exists.**
   Query the authoritative source for that literal value:
   the registry's tag list checked for the full tag, the
   package index for the exact name and release, the
   action's repository for the exact ref, the crate or
   package index for the exact name and version, the
   vendor's reference for the exact endpoint or key. A
   filtered, truncated or remembered listing is not a
   check.
2. **Open a URL once when you add it to a document.**
   Offline link checks skip external URLs, so nothing else
   will. An existing URL is not rechecked on every edit.
3. **Say what you verified and how.** The handoff to the
   reviewer names every external identifier the change
   introduces and the source each one was checked against.

## Reviewer Backstop

A handoff that introduces an external identifier without
saying how it was verified is rejected: ask for the check,
not for a promise. The identifier may well be right; the
rule exists because nothing else in the pipeline will say
so when it is not.

## Why This Matters

A wrong identifier passes every gate. The diff is small
and self-consistent, the reviewer cannot tell
`ubuntu-26.04` from `ubuntu26.04` without the same query,
and the failure surfaces on someone else's machine,
outside the session that wrote it and without its
context. One query at writing time costs seconds; the
rebuild that fails costs the user's time and trust.
