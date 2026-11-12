title: DD#50 — Function free-read tracking
status: implemented

1. Check a function's global reads against the caller's incoming taint state.
2. Independent per-function summaries cover globals and collection slots;
   received taint cannot be attested inside the callee (DD127).

---

Track which globals each function body reads (free variable reads).
At call sites, check whether any of those globals are currently tainted.
~~Lightweight inter-procedural analysis — no function signatures.~~
DD127 supplies the independent summaries; DD81/DD100 define call signatures.
