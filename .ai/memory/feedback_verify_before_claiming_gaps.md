---
name: Verify existing capabilities before claiming gaps
description: Always check ServerCapabilities and handler implementations before listing LSP features as unimplemented
type: feedback
---

Before presenting LSP features as "gaps" or "follow-up work," verify against the actual codebase — grep `ServerCapabilities` in server.rs for capability registration and check for handler implementations. A surface-level scan of docs and feature-log is not sufficient; features may be implemented without being documented in feature-log.md.

**Why:** A production incident listed 5 already-implemented LSP features (definition, references, rename, folding, selection ranges) as unimplemented gaps, wasting the user's time on false choices and eroding trust in the research. The code was well-structured and a single grep would have caught all five.

**How to apply:** When auditing what a project supports, check the capability registration and handler code directly — don't infer from docs alone. For rlsp-yaml specifically, `server.rs:capabilities()` is the source of truth for what's implemented.
