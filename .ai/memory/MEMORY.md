# Memory Index

- [project_followup_plans.md](project_followup_plans.md) — Open items: feature work (#1-3), cleanup queue (C1-C4: stale refs, match refactors, iterator patterns)
- [potential-performance-optimizations.md](potential-performance-optimizations.md) — Parser perf (Option D, lazy spans) + loader candidates. Applied: L5, L2, L7, L1, L3 (+6.4% on block_heavy), L6 memchr fast-path, L4 scoped (leading_comments Option<Vec<String>>). Deferred: L4 full Option<Box<NodeMeta>>
