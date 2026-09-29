---
name: hardcoded-secret
description: A key, token or private key written into the code
severity: block
match: -----BEGIN (RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----
match: \bAKIA[0-9A-Z]{16}\b
match: \bsk-(live|test|proj|ant)-[A-Za-z0-9_-]{20,}
match: \bgh[pousr]_[A-Za-z0-9]{36,}\b
match: \bxox[abprs]-[A-Za-z0-9-]{10,}
match: \bAIza[0-9A-Za-z_-]{35}\b
exclude: *.example, *.sample, **/fixtures/**, **/testdata/**
---

A secret in the source is a secret in every clone, log, backup and history
entry, and removing it later does not take it out of the history.

Read it from the environment instead (`process.env.STRIPE_KEY`,
`std::env::var("STRIPE_KEY")`, `os.environ["STRIPE_KEY"]`), name the
variable in `.env.example` with a placeholder value, and keep the real value
in `.env`, which git ignores. A test uses an obviously fake value
(`sk_test_example`) or a fixture outside the code.
