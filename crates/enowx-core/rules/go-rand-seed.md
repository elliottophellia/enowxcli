---
name: go-rand-seed
description: `rand.Seed` is deprecated
severity: remind
match: \brand\.Seed\(
files: *.go
---

Since Go 1.20 the global source is seeded automatically; remove the call.
For reproducible values make a local generator:
`r := rand.New(rand.NewPCG(1, 2))` with `math/rand/v2`.
