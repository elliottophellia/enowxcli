---
name: react-dangerous-html
description: `dangerouslySetInnerHTML` renders raw HTML
severity: remind
match: dangerouslySetInnerHTML
files: *.tsx, *.jsx
---

Raw HTML from anywhere a user can reach is cross-site scripting. Render the
content as elements, or sanitise it first (DOMPurify) and say where it comes
from in a comment.
