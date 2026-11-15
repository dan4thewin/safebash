# 632 - A loop binder takes the element type

While tightening the tour's collection-slot slide, `(( sum += v ))` over an
`int array` raised SB4030. The `for v in @a` binder was always a string, even
though a direct cell read and `v=${a[0]}` already produce an `int`. No DD
settled the binder's type. The AI offered to give the binder the element type
(consistent with DD76 and DD123), or to document the binder as string-only.
DG chose: **"yes to loop var takes type; needs a change to the spec outline
too"** (2026-11-13).

[DD134](../decisions/DD134-loop-binder-takes-element-type.md) records the rule:
the binder is an assignment from the current cell. A fresh binder takes the
element type. An existing binder of the other type is SB4030, as `s=${a[0]}`
is for a `str` today; the AI applied this consequence from the assignment
principle rather than inventing a new exception. Other iteration sources
still bind strings. DD76's rule list points to DD134. The spec outline's
named-collection iteration item now says so. Implementation is queued in
[loop-binder-element-type](../tasks/loop-binder-element-type.md); the tour
keeps `${v:-0}` until it lands.
