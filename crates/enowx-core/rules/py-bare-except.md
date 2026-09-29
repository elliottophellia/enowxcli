---
name: py-bare-except
description: A bare `except:` also catches Ctrl+C and exits
severity: remind
match: ^\s*except\s*:
files: *.py
---

A bare `except` catches `KeyboardInterrupt` and `SystemExit` too, so the
program cannot be stopped and real errors vanish. Catch what you expect
(`except (ValueError, KeyError) as error:`), or `except Exception` at a top
level that logs and re-raises.
