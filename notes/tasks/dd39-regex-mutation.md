title: Regex mutation and substitution operators
status: done
severity: feature
gates: [39.2, 39.3, 39.4]

DD#39: var =~ s/re/str/ mutates in place, ${var:s/re/str/} returns
without mutating, ${var/pat/rep} accepted via glob-to-regex coercion.
