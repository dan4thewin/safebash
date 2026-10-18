title: Adversarial audit run-3
status: deferred
depends: []
gates:
  - commit

Schedule after next significant feature work. Run-2 artifacts in
`audit/run-2/`. Process: focused prompts → 5+ diverse LLM models →
triage → deduplicate → remediation → reconcile → implement.

Coverage 525 adds a concrete review seam: centralized lexical identities
(`bindings.rs`) and one inferred full collection signature per function.
Include sibling/nested capture isolation, inherited no-shadow, name-taking
operands versus literal values/keys, forwarded slots and fixed slot counts,
all four collection types, and indexed/whole `unset` writeback. Check both
front-end taint and the independent SSA backstop after resolution; source-name
restoration must not affect the identities used to classify taint.
Journal 525 records the controls and ordinary/coverage gates; those are not
a substitute for the adversarial review.
