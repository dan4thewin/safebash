title: DD#37 — Reject process substitution
status: implemented

`<(cmd)` and `>(cmd)` rejected (SB5004). Coerce to pipe-while-read
or explicit mkfifo with named pipes.
