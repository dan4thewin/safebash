title: DD#40 — ShellCheck as lint gate
status: implemented

1. The converter uses ShellCheck as an internal analysis tool, not a
   precondition; native compilation has no Bash lint gate (DD57).

---

~~ShellCheck runs first by default (`--nolint` to skip). Validator reduced to
subset enforcement only — constructs sash cannot compile. Quoting rules
not implemented (sash's value model makes `$var` and `"$var"` identical).~~

DD57 supersedes the shared-tool lint architecture; DD30 distinguishes quoting
as an explicit integer-to-string conversion.
