title: DD#52 — Remove ${var@...} parameter transforms
status: implemented

`${var@U}`, `${var@u}`, `${var@L}`, `${var@Q}`, `${var@E}` rejected (SB4015).
Use `${var^^}` / `${var,,}` instead. Cleaner subset.
