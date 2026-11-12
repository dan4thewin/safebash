title: Word splitting elimination — runtime semantics
status: done
severity: correctness
gates: [22.1, 22.4, 22.5, 22.6, 22.7, 28.2, 40.3]

DD#22/DD#28/DD#40: $var and "$var" identical, read/mapfile
stream semantics, for-in with array expansion, IFS doesn't exist.
22.2 (IFS rejection) and 22.3/28.1 (for-in $var rejection) are
validator concerns — separate from runtime semantics.
