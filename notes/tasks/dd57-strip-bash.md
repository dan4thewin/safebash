title: sash compiler — strip bash concerns
status: done
severity: feature
gates: [57.42a, 57.42b, 57.43, 57.44, 57.45, 57.46]
---

Phase 2 of DD#57 split experiment.

- Add catchall rule for non-sash node types (57.44)
- Add non-sash declaration keyword rejection (57.45)
- Remove --emit-source, --annotate, --no-lint flags (57.46)
- Remove validator/rules/known_externals from sash
