---
layout: default
title: Documentation and agent index
---

# Documentation and agent index

`ConchFeng/RedAlert2` develops VERA20k, a Rust reimplementation of the Yuri's
Revenge engine using player-supplied retail assets. The current code determines
behavior; documentation explains its owners and how to work with them.

## Start here

1. Read [AGENTS.md](../AGENTS.md) for scope, architecture, evidence and validation
   requirements. Explicit user instructions take precedence.
2. Read the [project guide](project-guide.md) for the four workspace crates,
   production entry points, frame flow and state ownership.
3. Use the [README](../README.md) for setup and player-facing status, or the
   [developer tools index](../tools/README.md) for a concrete command.

The project guide links to the production source for each claim. Module-level
`//!` headers give local responsibilities and dependency boundaries; read the
affected owner and its callers before making a change.

## Find a guide by task

| Task | Documentation and source starting points |
| --- | --- |
| Understand the code or plan a change | [Project guide](project-guide.md), [simulation](../src/sim/mod.rs), [application](../src/app/mod.rs). |
| Build, test or resolve an executable | [Developer tools](../tools/README.md), [capability registry](capabilities.md). |
| Configure retail files or MOD roots | [INI profiles](ini-roots.md), [source selection](../src/rules/retail_sources.rs), [config example](../config.toml.example). |
| Change a rule, default or parser | [INI lookup](../tools/ini_lookup.md), [native value readers](../src/rules/ini_value.rs), [directives](ini-directives.md). |
| Configure player rule overrides | [Personal rules](personal-rules.md), [commander personalities](ini-commander-personalities.md). |
| Inspect or render original assets | [Asset browser](../tools/asset_browser/README.md), [media selection](../tools/media_policy.md). |
| Work on gameplay, ordering or saves | [Project state and frame flow](project-guide.md#state-ownership-and-frame-flow), [simulation snapshots](../src/sim/snapshot.rs). |
| Change LAN, online play or the relay | [Networking](Networking.md), [lobby service contract](lobby-protocol.md), [relay service](../services/relay/README.md). |
| Work on browser builds | [WASM preview](wasm.md), [browser networking and LAN](wasm-net.md), [gateway](../tools/wasm/gateway/README.md). |
| Integrate an external launcher | [Integration index](../integrations/README.md), [CnCNet single-player setup](../integrations/cncnet/README.md), [spawn adapter](../src/app/frontend/cncnet_spawn.rs). |
| Reproduce a map or inspect match output | [Map observation](../tools/map_observation.md), [test maps](capabilities.md#test-maps), [telemetry dashboard](../tools/telemetry-dashboard/README.md). |
| Compare original-engine behavior | [Native inspection](../tools/native_inspect.md), [native oracle policy](../tools/native_oracle.md), [Ghidra workflow](ghidra-workflow.md). |
| Package or publish an authorized build | [Release skill](../.agents/skills/release/SKILL.md), [release workflow](../.github/workflows/release.yml), [deployment prerequisites](../.github/release/downloads.md). |

## Gameplay extension references

Features are opt-in and do not establish support for an entire third-party mod.
Each overview links to its implemented chains and limitations.

| Feature | Guide |
| --- | --- |
| `ares` | [Ares](ares.md) |
| `phobos` | [Phobos](phobos.md) |
| `npatch` | [NPatch compatibility](npatch.md) |
| `verax` | [VeraX](verax.md) |
| `npbug` | [NPBUG capability status](npbug.md) |

The [capability registry](../tools/capability_tests.py) owns feature combinations
and compile/test selectors. Consult it instead of maintaining another feature
list in a task prompt.

## Collaborating agents

- Name the behavior and existing owner before assigning work. Give independent
  tasks distinct files or owners and identify any shared boundary explicitly.
- Share the requirement, allowed mutations and expected result. A research or
  review assignment does not authorize implementation or publication.
- Re-read shared files and Git state before editing. Other agents' changes and
  staged work are live; preserve both and coordinate before changing a shared API.
- Use the [coding conventions](../.agents/skills/code-style/SKILL.md) and
  [submission standards](../.agents/skills/code-submission/SKILL.md). Full builds
  and commits require the user's authority described in AGENTS.md.
- Keep source and Git staging stable while the build runner validates a command.
  Choose focused checks for the affected owner rather than duplicating build work.
- For sustained work, maintain one concise [checkpoint](../.agents/skills/_shared/handoff.md)
  with actual state, scope, validation and the next safe action. Use an
  [independent review](../.agents/skills/_shared/review.md) when requested or
  justified by the change's risk.

## Language maintenance

Documentation and explanatory comments use American English. Localization values,
Unicode regression inputs, retail bytes and original contributor names retain
their meaning and identity.

For a bulk cleanup, review exact prose replacements before applying a temporary
script. Distinguish comments and docstrings from executable strings, heredocs and
configuration values; matching a character range alone does not identify prose.
Check the resulting diff and confirm that runtime literals and executable content
retain their original bytes. Remove disposable scripts and their temporary test
files once the cleanup is verified.

For new or changed Python tools, follow AGENTS.md and run the repository Python
suite. For documentation-only edits, check claims, paths and links. Update this
index when a guide moves; keep detailed behavior with its current owner.
