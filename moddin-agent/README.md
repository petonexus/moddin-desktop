# moddin-agent

MCP server and agent skill that lets an AI assistant **add new mods to Moddin
Desktop** without the user writing any YAML by hand.

The user talks in natural language ("add this FPS unlocker to Elden Ring").
The agent asks the few questions it cannot guess, drafts a **capability
recipe** in Moddin's YAML contract, validates it, and drops the file into
`%LOCALAPPDATA%\Moddin\capabilities\`. The user then opens Moddin Desktop,
sees the new capability in the UI, and clicks **Apply** — Moddin shows the
preview, runs the install, and records a rollback transaction as usual.

> 🛡️ **Safety boundary.** The agent never touches game files or the registry.
> Moddin Desktop stays the source of truth for every destructive action.
> The agent is an **authoring assistant**, not an installer.

## Install

```powershell
cd moddin-agent
npm install
```

Requires Node.js 20+ (the Moddin Desktop dev environment already ships it).

## Register with your agent

The server speaks MCP over **stdio**. Add it to your agent's MCP config
(Claude Desktop, Claude Code, Codex, Cursor, etc.):

```jsonc
{
  "mcpServers": {
    "moddin": {
      "command": "node",
      "args": ["C:/mods/moddin/moddin-agent/src/mcp-server.mjs"],
      "env": {
        "MODDIN_PROJECT_ROOT": "C:/mods/moddin/moddin",
        "MODDIN_LOCAL_CAPABILITIES_DIR": "" // optional; defaults to %LOCALAPPDATA%/Moddin/capabilities
      }
    }
  }
}
```

Restart the agent. The `moddin` MCP server will expose its tools (see
`SKILL.md`) and the agent will follow the workflow on its own.

## What the agent can do

| Tool | Purpose |
| --- | --- |
| `list_supported_games` | Enumerate the games Moddin's built-in catalog recognises (engine, executable, install layout). |
| `get_game_info` | Deep-dive on one game: supported engines, executable relative path, modules list. |
| `list_capabilities` | List every capability currently shipped + locally. |
| `get_capability` | Read one capability recipe. |
| `get_step_kinds` | Enumerate the install/uninstall step kinds with their required params. |
| `get_check_kinds` | Enumerate the pre-flight / verify check kinds. |
| `get_capability_template` | Return a starter YAML for a chosen pattern (extract-zip, proxy-dll, registry-override, vr-runtime-overlay). |
| `validate_capability_yaml` | Parse + schema-validate a draft YAML; return the list of errors. |
| `preview_capability_plan` | Static dry-run: predict the files / registry keys / processes a draft will touch, **without** executing anything. |
| `save_capability_yaml` | Write a validated YAML to the user's local capabilities folder so Moddin picks it up at next launch. |

Every `save_capability_yaml` call writes to `SpecOrigin::Local` — Moddin
trusts the local file but still runs its own validation, preview, and
transaction at install time.

## End-to-end example

User prompt (in the agent):

> "I just downloaded `C:/Users/me/Downloads/FPSUnlocker.zip` and want it for
> Elden Ring."

The agent will:

1. Call `list_supported_games` → confirms `elden-ring` exists.
2. Call `get_game_info({ gameId: "elden-ring" })` → gets engine, executable path.
3. Call `get_capability_template({ pattern: "extract-zip" })` → starter YAML.
4. Fill in the missing fields (SHA-256 is still unknown — ask the user).
5. Compute SHA-256 of the user's file locally and produce the final YAML.
6. Call `validate_capability_yaml` → returns `valid: true`.
7. Call `preview_capability_plan` → returns the file list that will be created
   under the game's executable directory.
8. Show the user a one-paragraph summary + the predicted file list and ask
   for **explicit confirmation**.
9. On confirmation, call `save_capability_yaml` → writes
   `%LOCALAPPDATA%/Moddin/capabilities/fps-unlocker.yaml`.
10. Tell the user: "Open Moddin Desktop, click **Rescan**, the new
    capability **FPS Unlocker** will appear under Elden Ring. Click Apply."

## Design constraints

These are non-negotiable — they match the project rules in
`docs/CAPABILITY-CONTRACT.md` and the threat model in
`src-tauri/src/community_catalog.rs`:

1. The agent **never** invokes install/uninstall. Moddin UI does, after the
   user reviews the preview.
2. Every YAML is **schema-validated** before save. Invalid YAML is rejected
   locally, never persisted.
3. The agent **never** writes to the game directory, the registry, or any
   path outside `%LOCALAPPDATA%\Moddin\capabilities\` (and its own cache).
4. New step / check kinds require a Rust change. The agent flags
   `unsupported_kind` errors with a clear pointer to
   `docs/CONTRIBUTING.md`.

## See also

- `SKILL.md` — the agent-side system prompt that wires this MCP into the
  "user asks for a mod, agent delivers a capability" workflow.
- `schema/capability.schema.json` — JSON Schema mirror of Moddin's capability
  contract.
- `templates/*.yaml` — starter recipes the agent composes new specs from.
- `docs/AGENT-RECIPES.md` — architecture notes for maintainers.