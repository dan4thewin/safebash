title: Convert codegen panics to diagnostics
status: ready
depends: []
gates:
  - tests
  - commit

32 ICE-prefixed panics in codegen.rs. Several are unimplemented
features rather than true internal errors. Each is a UX cliff —
either implement the feature or surface as a Diagnostic upstream
so codegen never sees them.
