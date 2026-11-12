title: Positional arity checking with shift tracking
status: done
severity: correctness
gates: [48.1, 48.2]

DD#48: compiler tracks cumulative shift count when detecting
positional parameter range. 48.1 (basic arity) is gated by
dd46-reject-toplevel-positionals.
