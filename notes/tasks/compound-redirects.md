title: Compound command redirects
status: done
severity: feature
gates: [14.4, 14.5, 14.6]
---

Redirects on compound commands: for...done > file, while...done | cmd,
if...fi > file. Codegen has partial support but edge cases remain.
