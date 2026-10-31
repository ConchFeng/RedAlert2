---
name: code-style
description: Apply RedAlert2 coding conventions when adding, changing or reviewing Rust, C++ or Python code.
---

# Code style

Use the existing owner and nearby conventions. Keep changes focused on the requested
behavior; the architecture and native-evidence requirements live in
[AGENTS.md](../../../AGENTS.md).

- Write code, identifiers and documentation in American English. Prefer names that
  explain intent, small cohesive functions and direct control flow.
- Reuse the authoritative state and shared decisions. Complete affected migrations
  and remove superseded code rather than adding caller-specific copies.
- Match native gameplay through the existing readers, deterministic math and tick
  phases. Keep native function names, addresses and source beside derived Rust
  behavior; preserve necessary safety and numeric rationale.
- Use Rust naming conventions: `snake_case` for functions/modules/variables,
  `PascalCase` for types/traits and `SCREAMING_SNAKE_CASE` for constants.
- Keep ownership and mutation explicit. Use `Result` for expected failures and
  `Option` for absence. Avoid unchecked panics on user files or network input.
- Add types, traits, shared pointers or dependencies only when actual consumers
  need them. Choose allocation and dispatch from the workload; do not impose a
  global allocator, test-double pattern or wrapper ban as a style rule.
- Share core test support through the existing `test-support` feature. Regression
  tests exercise production behavior and fail without the fix; avoid mirrored
  implementations and unnecessary parameter sweeps.
- Write C++ without comments and keep each C++ line within 240 characters. Use
  meaningful names, RAII and explicit ownership; retain the repository's language
  standard rather than introducing a new one for style alone.
- Keep Python tools compatible with their documented minimum version and reuse
  existing owners in `tools/`. Do not copy helpers into a new script.
- Format only edited Rust leaf files with `rustfmt --edition 2024 <file>`; do not
  recursively format `mod.rs` or unrelated files.

Validate the affected behavior using the narrowest relevant check in AGENTS.md.
Build the entire project only when explicitly requested.
