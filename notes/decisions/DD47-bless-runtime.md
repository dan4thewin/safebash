title: DD#47 — Bless runtime enforcement
status: implemented

`bless` is a first-class SSA instruction with codegen. Test expression
evaluated at runtime; on failure, writes "bless: validation failed"
to stderr and exits 1.
