title: DD#52 — Remove ${var@...} parameter transforms
status: implemented

1. Case conversion uses the explicit `toupper`/`tolower` intrinsics; parameter
   expansion does not provide a separate transform family (DD65).

---

`${var@U}`, `${var@u}`, `${var@L}`, `${var@Q}`, `${var@E}` rejected (SB4015).
~~Use `${var^^}` / `${var,,}` instead.~~ DD65 moves these to `toupper` and
`tolower`. Cleaner subset.
