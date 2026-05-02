title: DD#33 — contradictory key representation
status: resolved
source: claude-opus-4.6, gpt-5.4
severity: accuracy

Resolved: Both claims were correct but described different cases.
Integer keys (indexed arrays) are hashed as raw 8-byte i64 — codegen
passes `(&i64, 8)` via `is_integer_key` flag on SubscriptAssign.
String keys (associative arrays) are passed as `(ptr, len)`. The DD
note was confusingly written, not contradictory. Updated to distinguish
the two cases.
