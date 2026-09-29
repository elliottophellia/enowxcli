---
name: go-exp-slices
description: `golang.org/x/exp/slices` or `maps` where the standard library has them
severity: remind
match: \"golang.org/x/exp/(slices|maps)\"
files: *.go
---

Since Go 1.21 use the standard `slices` and `maps` packages. Note that
standard `maps.Keys` and `maps.Values` return iterators: collect with
`slices.Collect(maps.Keys(m))`.
