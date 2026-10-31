# Independent review

Use an independent read-only critic when requested, or when a risky gameplay change
would benefit from a separate investigation. Routine docs, tooling and small fixes
do not need an automatic critic pass.

Give the reviewer the requirement, diff, affected owners, relevant native evidence
and actual validation results. The reviewer reads the affected consumers and
reports concrete defects, prioritizing player-visible behavior, determinism, state
authority, lifecycle and regression coverage. Names and decompiler annotations are
leads; native claims require the instructions and retail layers AGENTS.md specifies.

Keep the pass read-only: no edits, Git mutations, builds, native execution or Ghidra
writes. Report missing evidence or needed runtime checks as coverage gaps.

Each finding states its trigger, consequence and source. Separate confirmed defects
from suspicions and architectural preferences. Stop when further checking is
unlikely to change the result; do not repeat critics after routine fixes unless a
new risk or user request justifies it.

The owner evaluates findings, fixes in-scope defects and runs affected validation.
Review does not grant commit, publication or merge authority.
