title: DD#50 — Function free-read tracking
status: implemented

Track which globals each function body reads (free variable reads).
At call sites, check whether any of those globals are currently tainted.
Lightweight inter-procedural analysis — no function signatures.
