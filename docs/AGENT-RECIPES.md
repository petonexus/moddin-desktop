# Moddin Agent — agent-driven capability authoring

This document is the architecture note for the **agent layer** that sits
on top of Moddin Desktop. It explains what problem the agent solves,
what it deliberately does *not* do, and how the pieces fit together.

If you only want to **use** the agent, read [`moddin-agent/SKILL.md`](../moddin-agent/SKILL.md)
and [`moddin-agent/README.md`](../moddin-agent/README.md) instead.

## The problem

Moddin's runtime is already data-driven: every mod ships as a
**capability recipe** — a single YAML file under
`src-tauri/capabilities/<id>.yaml` (built-in) or
`%LOCALAPPDATA%/Moddin\capabilities\<id>.yaml` (user-local). The schema
in [`docs/CAPABILITY-CONTRACT.md`](CAPABILITY-CONTRACT.md) covers
download, extract, hash-verify, write text / binary files, move files,
spawn / kill processes, and HKCU registry edits. It is the complete
vocabulary of the runner.

But the schema is the wrong surface for a non-developer. They want to
say *"add this FPS unlocker to Elden Ring"* and have a working mod
land in the UI. Hand-writing YAML, picking the right step kinds, and
guessing SHA-256s is where people drop off.

The agent layer closes that gap.

## What the agent is

A small MCP server (`moddin-agent/`) that exposes the Moddin catalog,
the capability schema, dry-run previews, and a single write tool —
`save_capability_yaml`. It is paired with a skill (`moddin-agent/SKILL.md`)
that walks the agent through the workflow: understand intent, pull the
right template, fill in user-supplied config, validate, preview, get
explicit confirmation, save.

## What the agent is not

- It is **not** an installer. It never invokes install / uninstall /
  rollback. Those go through Moddin's UI by the user's hand, with the
  full preview / transaction / activity log.
- It is **not** a sandbox escape. It can only write to
  `%LOCALAPPDATA%/Moddin/capabilities/` — the same folder Moddin itself
  reads. It cannot touch the game directory, the registry, or anything
  else.
- It is **not** a replacement for community contributions. The
  community catalog ([`petonexus/moddin-community-capabilities`](https://github.com/petonexus/moddin-community-capabilities))
  is still the right place for capabilities other users will rely on.
  The agent is for **personal** / **first-run** recipes.

## Component layout

```
┌──────────────────────────────────────────────────────────────┐
│                     User's AI agent                          │
│  (Claude Desktop / Claude Code / Codex / Cursor / …)          │
│                                                              │
│   1. Reads SKILL.md                                          │
│   2. Talks to user in natural language                       │
│   3. Calls moddin MCP tools to inspect / draft / validate     │
└────────────────┬─────────────────────────────────────────────┘
                 │ MCP stdio
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                   moddin-mcp (Node.js)                       │
│                                                              │
│   tools:                                                     │
│     list_supported_games  get_game_info                      │
│     list_capabilities     get_capability                     │
│     get_step_kinds        get_check_kinds                    │
│     get_capability_template                                  │
│     validate_capability_yaml                                 │
│     preview_capability_plan                                  │
│     save_capability_yaml   (→ writes to local dir)            │
└────────────────┬─────────────────────────────────────────────┘
                 │ reads YAML files
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                Moddin Desktop (Tauri 2)                      │
│                                                              │
│   src-tauri/capabilities/      ← built-ins (compiled in)     │
│   %LOCALAPPDATA%/Moddin/capabilities/ ← user-local (MCP)     │
│   src/catalog/games/*.yaml     ← game catalogue              │
│                                                              │
│   On launch:                                                 │
│     • loads every local YAML as SpecOrigin::Local            │
│     • user opens the game → sees new capability              │
│     • clicks Apply → preview → backup → install              │
│       → verify → activity log                                │
└──────────────────────────────────────────────────────────────┘
```

## How a capability is added end to end

1. **Intent** — the user tells the agent what they want in plain
   language. The agent parses game + pattern + source.
2. **Game lookup** — `get_game_info` confirms the game is supported and
   returns the engine(s) + executable relative path.
3. **Template pull** — `get_capability_template` returns a starter
   YAML for the matching pattern (`extract-zip`, `proxy-dll`,
   `registry-override`, `vr-runtime-overlay`).
4. **Config fill** — the agent collects missing fields (URL, SHA-256,
   paths) from the user. SHA-256 may be computed locally from a file
   the user provides.
5. **Compose** — the agent produces a single YAML text that fills the
   template.
6. **Validate** — `validate_capability_yaml` runs the JSON Schema.
   Errors come back with the exact path; the agent fixes and retries.
7. **Preview** — `preview_capability_plan` does a static walk over the
   YAML and produces a human-readable plan (files, registry keys,
   processes, blocker checks).
8. **Confirm** — the agent restates the plan and asks the user to
   confirm.
9. **Save** — `save_capability_yaml` writes the validated text to
   `%LOCALAPPDATA%/Moddin/capabilities/<id>.yaml`.
10. **Apply** — the user opens Moddin, picks the game, and clicks
    **Apply**. Moddin runs its own preview / backup / install / verify
    pipeline as if the capability had been written by hand.

## Threat model

The agent is bounded by three rules that map directly to Moddin's
existing trust model:

| Risk | Mitigation |
| --- | --- |
| Agent crafts YAML that breaks the game | Every save goes through `validate_capability_yaml` against the JSON Schema mirror of the Rust contract. Invalid YAML is rejected locally, never persisted. |
| Agent saves a malicious YAML the user did not see | `save_capability_yaml` requires an explicit confirmation gate in the workflow (the skill enforces it). The `preview_capability_plan` output is shown before the save call. |
| Agent bypasses the local-capabilities sandbox | The MCP server has no other write tool. There is no `touch_game_dir`, `write_registry`, or `spawn_process` tool. The transport is stdio only; no inbound HTTP. |
| Agent races Moddin mid-install | The agent never calls `capability_install` / `capability_uninstall`. It only writes the YAML; Moddin reads it on the next reload. |
| Local capability shadows a built-in | Moddin's `SpecOrigin::Local` precedence rule applies: a local capability with the same `id` overrides the built-in. The user must opt in by saving the file. The agent surfaces this in the preview when it detects an `id` clash. |
| Capability uses a kind the runner does not support | The schema rejects unknown kinds at validate time. The skill refuses to proceed when the user asks for behaviour outside the enum. |

## Why not give the agent install access?

It is tempting — the Tauri commands `capability_install` /
`capability_uninstall` already exist. We deliberately do **not** expose
them to the agent because:

- The **preview** the user sees must be Moddin's preview, not the
  agent's. Moddin's preview integrates with the file tree, registry
  history, transaction store, and activity log. Anything the agent does
  in parallel risks showing the user a stale plan.
- The **transaction** is Moddin's. Without going through
  `TransactionRecord`, there is no rollback — the entire promise of
  "Moddin remembers what it did" breaks.
- The **trust boundary** is Moddin's. The community catalog is signed
  by the maintainer key; the agent is not. If the agent could install
  unsigned code, every Moddin user who runs an agent gets a free pass
  on the same threat the project spent real effort closing.

If a future need genuinely requires the agent to apply (e.g. a CLI-only
headless mode), the right answer is a **new Tauri command with a
separate UI confirmation** — not a wider MCP surface.

## Files in this folder

| File | Purpose |
| --- | --- |
| `moddin-agent/package.json` | Node.js package; depends only on `@modelcontextprotocol/sdk`, `ajv`, `ajv-formats`, `yaml`. |
| `moddin-agent/src/mcp-server.mjs` | The MCP server. Single ESM file. No build step. |
| `moddin-agent/schema/capability.schema.json` | JSON Schema mirror of the Rust `CapabilitySpec`. Source of truth for `validate_capability_yaml`. |
| `moddin-agent/templates/*.yaml` | Starter recipes for the four supported patterns. The agent fills them in. |
| `moddin-agent/SKILL.md` | The system prompt the agent loads alongside the MCP server. Walks it through the workflow. |
| `moddin-agent/README.md` | Quick start: install, register with the agent, point at the Moddin project. |

## Cross-references

- [`docs/CAPABILITY-CONTRACT.md`](CAPABILITY-CONTRACT.md) — the
  authoritative capability spec.
- [`docs/CONTRIBUTING.md`](CONTRIBUTING.md) — how to add a new step /
  check kind (the only case where the agent asks the user to file a
  PR).
- [`moddin-agent/SKILL.md`](../moddin-agent/SKILL.md) — the agent-side
  system prompt.
- [`moddin-agent/README.md`](../moddin-agent/README.md) — install /
  register.