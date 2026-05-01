title: DD#40 — ShellCheck as lint gate
status: implemented

ShellCheck runs first by default (`--nolint` to skip). Validator reduced to
subset enforcement only — constructs sash cannot compile. Quoting rules
not implemented (sash's value model makes `$var` and `"$var"` identical).
