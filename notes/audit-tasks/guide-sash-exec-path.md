title: GUIDE.md — sash_exec PATH lookup vs DD#23 compile-time PATH
status: ready
source: gpt-5.4
severity: accuracy

GUIDE.md Runtime section says sash_exec does "PATH lookup." DD#23 says
PATH is compile-time only. Clarify what sash_exec actually does — does
it use a compile-time-resolved absolute path, or does it search PATH
at runtime?
