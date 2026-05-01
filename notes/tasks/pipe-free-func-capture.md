title: Pipe-free user-function capture
status: ready
depends: []
gates:
  - tests
  - commit

`$(myfunc)` currently redirects stdout through a pipe even though
the function body is inlined. Should accumulate into a buffer
directly, eliminating the pipe/read overhead. External commands
still need fork+pipe. This is the trampoline model's (DD#27)
killer advantage — the compiler can see inside the function.
