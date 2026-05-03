title: DD#25 — overflow traps designed but spec presents arithmetic without caveats
status: resolved
source: gpt-5.4
severity: gap

DD#25 overflow traps are only "designed." SUBSET.md presents arithmetic
as accepted without caveating runtime trap semantics or string→int
conversion failure modes. Arithmetic remains under-specified for a
language that prides itself on removing bash ambiguity.
