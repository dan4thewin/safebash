title: Bless semantics need single canonical document
status: resolved
source: gpt-5.4
severity: gap

Resolved: DD#24 is now the canonical taint/bless document, absorbing
DD#43, DD#44, DD#47, DD#49. All questions answered: two runtime forms
(fatal and conditional), test expression is pure [[ ]] (no commands,
no arithmetic), other tainted vars not readable in test. DG confirmed.
