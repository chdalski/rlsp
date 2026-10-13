# Code Navigation with Language Servers

Before you search code for an identifier — a function,
type, field, variable, or module name — ask the `LSP` tool,
not Grep. Grep matches text, not symbols: it misses
re-exports, aliases, and trait or interface dispatch, and
it returns unrelated symbols that share a name. A signature
change or impact assessment built on grep results silently
misses call sites or touches the wrong ones.

`LSP` is the default, not an option to weigh: use Grep for
an identifier only after `LSP` reports that no server
covers its file type (see Availability). A prior lead kept
searching with Grep while this rule was loaded, until the
user asked for `LSP`.

## Availability

- `LSP` exists only when a code intelligence plugin for
  the language is enabled and its server binary is on
  `PATH`. If `LSP` is neither in your tool list nor named
  in a deferred-tool list, use Read and Grep.
- If `LSP` appears only by name in a deferred-tool list,
  load it with `ToolSearch` (`select:LSP`) the first time
  you are about to search for an identifier. A deferred
  tool has no schema until loaded, so it is easy to
  overlook — load it instead of falling back to Grep.
- The first calls after the server starts can return "No
  references found" while it indexes. Retry before
  concluding a symbol is unused — an empty result from an
  unindexed server is not evidence.
- An error that no server is configured for the file type
  means that language has no server: use Grep for those
  files without retrying.

## Which Tool for Which Question

| Question | Use |
|---|---|
| Where is this symbol defined? | `LSP` `goToDefinition` |
| Every usage — before renaming, deleting, or changing a signature | `LSP` `findReferences` |
| Who calls this function — impact of a behavior change | `LSP` `incomingCalls` |
| Type or documentation at a position | `LSP` `hover` |
| Implementations of an interface or trait | `LSP` `goToImplementation` |
| A symbol by name across the workspace | `LSP` `workspaceSymbol` with a non-empty `query` |
| Strings, log messages, comments, config keys, non-code files | Grep |

Operations take a 1-based `line` and `character`. Read the
file first and take the position from it — a guessed
position lands on whitespace or a neighboring token and
returns nothing.

## Renames and Signature Changes

Rename a symbol or change its signature through its
references: resolve every site with `findReferences` — use
`hover` where a site's type is unclear — and change exactly
those sites with Edit. Never rename across files with sed,
perl, or a regex script, however many sites there are: it
renames text, not the symbol. A prior session renamed a
field with a regex script; it also renamed integer values
and test-helper parameters that shared the name, missed a
`_`-prefixed variant, and the wrong renames still
compiled. The build confirms the result; it does not find
the sites.

## Diagnostics After Edits

In some sessions the tool result after an edit carries a
`<new-diagnostics>` block listing errors the edit caused,
including errors in other files that use the changed code.
Fix those errors before running tests or handing off.
Their absence proves nothing: other sessions receive no
diagnostics at all, and language servers can report false
unresolved-import errors in monorepos. The project's build
and test commands remain the verification.
