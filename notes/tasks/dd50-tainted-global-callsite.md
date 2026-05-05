title: DD#50 — tainted global at function call site
status: ready
severity: correctness
gates: [50.1]
---

If a function reads a global variable that is tainted at the call site,
compiler SHALL emit an error.
