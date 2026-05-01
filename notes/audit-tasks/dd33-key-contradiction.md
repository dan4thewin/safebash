title: DD#33 — contradictory key representation
status: ready
source: claude-opus-4.6, gpt-5.4
severity: accuracy

Note says "String keys always (`arr[5]` → key `"5"`)" then says
"Integer keys hashed as raw i64, not converted to strings." These
contradict each other. The amendment superseded the string-keys-always
rule — fix the note to reflect final state.
