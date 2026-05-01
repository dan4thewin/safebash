title: $$ and $! marked clean — potentially attacker-influenced
status: ready
source: gemini-pro
severity: gap

$$ (PID) and $! (last background PID) are classified as clean in
the taint model. In contexts where an attacker can influence process
creation order or timing, these could be low-bandwidth channels.
Evaluate whether this is an acceptable residual risk.
