---
name: go-ioutil
description: `io/ioutil` is deprecated
severity: remind
match: \"io/ioutil\"
files: *.go
---

Since Go 1.16 its functions live in `io` and `os`: `os.ReadFile`,
`os.WriteFile`, `os.ReadDir`, `os.MkdirTemp`, `os.CreateTemp`, `io.ReadAll`,
`io.Discard`, `io.NopCloser`.
