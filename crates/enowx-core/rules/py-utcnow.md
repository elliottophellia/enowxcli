---
name: py-utcnow
description: `datetime.utcnow()` is deprecated and returns a naive time
severity: remind
match: datetime\.utcnow\(\)
match: datetime\.utcfromtimestamp\(
files: *.py
---

Since Python 3.12 these are deprecated: they return times with no zone,
which compare wrongly with aware ones. Use `datetime.now(timezone.utc)` and
`datetime.fromtimestamp(ts, timezone.utc)`.
