---
name: py-mutable-default
description: A mutable default argument is shared between calls
severity: remind
match: def\s+\w+\([^)]*=\s*(\[\]|\{\}|set\(\)|dict\(\)|list\(\))
files: *.py
---

The default list or dict is created once and every call that leaves the
argument out shares it. Default to `None` and create it inside:
`def add(item, into=None): into = [] if into is None else into`.
