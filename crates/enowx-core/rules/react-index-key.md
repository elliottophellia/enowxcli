---
name: react-index-key
description: A list keyed by its index
severity: remind
match: key=\{\s*(i|idx|index)\s*\}
files: *.tsx, *.jsx
---

An index key makes React reuse the wrong element when the list is
reordered, filtered or added to: inputs keep another row's text, animations
jump. Key by the item's stable id (`key={order.id}`). An index is fine only
for a static list that never changes order.
