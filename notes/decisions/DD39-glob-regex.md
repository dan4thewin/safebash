title: DD#39 — Star-only glob + regex mutation
status: implemented

Star-only glob for files (`*` only, no `?` or `[abc]`).
Perl-style regex for string mutation: `var =~ s/re/str/flags` (statement, mutates).
Expression form: `${var:s/re/str/flags}` (returns value, no mutation).
`${var/pat/rep}` family accepted via glob→regex coercion.
