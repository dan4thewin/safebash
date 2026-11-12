title: DD#10 — Reject bash -c
status: implemented

1. A runtime string must not become executable shell code; command execution
   must remain statically analyzable.

---

`bash -c` rejected. Dynamic code execution — a string becomes code at
runtime. Incompatible with static analysis and the safety guarantees
safebash provides.
