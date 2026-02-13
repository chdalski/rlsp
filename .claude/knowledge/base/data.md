# Data Guidelines

## Single Source of Truth (SSOT)

Every piece of data should have one authoritative source.
All other references derive from that source.

## Applying SSOT

- Define one authoritative location for each data type
- Components query or subscribe to the source, don't copy
- Compute derived data from the source, don't store it
  separately
- Use references or identifiers instead of copying data
  structures
- Configuration values defined once, read by all components
- Caching is acceptable if invalidation is proper and
  the source is clear
