title: Sash-native subshell rejection — validator rule
status: done
depends: subshell-decomposition
severity: correctness
gates: [38.1]

Once subshell decomposition is implemented in the lowerer for bash-mode
input, add a validator rule that rejects `( cmd )` in sash-native mode.
Sash-native code should use the explicit primitives (local, functions,
fork via &) rather than the subshell abstraction.

Assign an SB diagnostic code.
