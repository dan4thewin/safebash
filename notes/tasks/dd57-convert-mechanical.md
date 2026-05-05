title: sash-convert — remaining mechanical conversions
status: ready
severity: feature
gates: [57.5, 57.6, 57.10, 57.13, 57.14a, 57.14b]
---

Phase 1 of DD#57 split experiment.

- Backtick → $() conversion (57.5)
- [ ] → [[ ]] conversion (57.6)
- $((x=10)) extraction to ((x=10)) (57.10)
- Top-level positional $1 → $(argv 1) (57.13)
- trap "" SIG → trap : SIG (57.14a, 57.14b)
