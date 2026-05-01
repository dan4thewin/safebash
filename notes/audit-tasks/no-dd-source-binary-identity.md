title: No decision on source/binary identity in chain of trust
status: ready
source: gpt-5.4
severity: gap

DD#32 and DD#45 imply a provenance story. But no decision covers:
is .sash_source the coerced source or original input? Is byte identity
between --emit-source and .sash_source part of the trust model? Is
signing intended to attest to coerced program, original bash, or both?
What prevents ambiguity when coercions normalize distinct inputs to
same output?
