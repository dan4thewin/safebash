title: DD#37 — Reject process substitution
status: implemented

1. Connect processes through explicit pipelines or named FIFOs rather than
   filename-like process substitution.

---

`<(cmd)` and `>(cmd)` rejected (SB5004). Coerce to pipe-while-read
or explicit mkfifo with named pipes.
