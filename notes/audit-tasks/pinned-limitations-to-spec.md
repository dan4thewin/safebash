title: Pinned limitations should be in SUBSET.md or decisions
status: resolved
source: gpt-5.4
severity: gap

notes/pinned.md has known limitations (regex {n,m}, [[ -v ]] edge case,
cranelift version) that are user-visible but absent from SUBSET.md
and decisions. If intentional, elevate to decisions. If accidental but
known, put in SUBSET.md as explicit current limitations. Pinned notes
while the public spec speaks in cleaner abstractions is how safety
systems lie to users.
