title: SUBSET.md — "rejected (coerced to...)" language is incoherent
status: ready
source: gpt-5.4
severity: contradiction

SUBSET.md says `[ ]` is "rejected (coerced to `[[ ]]`)" and backticks
are "rejected (coerced to `$(cmd)`)". Rejected and silently coerced
are mutually exclusive. The compiler accepts and transforms these —
they are coercions, not rejections. Fix the language throughout.
