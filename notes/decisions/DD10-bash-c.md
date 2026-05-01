title: DD#10 — Reject bash -c
status: implemented

`bash -c` rejected. Dynamic code execution — a string becomes code at
runtime. Incompatible with static analysis and the safety guarantees
safebash provides.
