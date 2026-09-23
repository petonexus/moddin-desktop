---
name: moddin
description: Add new mods to Moddin Desktop by authoring capability recipes in plain conversation. Use when the user wants a mod (FPS unlocker, trainer, overlay, VR runtime layer, registry tweak, proxy DLL) installed on a Steam/Epic game they already own. Drives an MCP server that validates and persists capability YAMLs to %LOCALAPPDATA%/Moddin/capabilities/. Never touches the game directory directly — Moddin Desktop previews and applies them with full transaction / rollback.
---

# moddin — add a mod to Moddin Desktop, end to end

Moddin Desktop is a Windows app that installs and rolls back game mods.
It already understands a **capability recipe** — a single YAML file that
declares what a mod does, what files it owns, and what to verify. The
user's job is normally to write that YAML by hand. Your job is to do it
for them, in conversation.

The `moddin` MCP server (registered alongside this skill) exposes the
catalog, the schema, dry-run previews, and the write step. You compose
new recipes from the bundled templates and validate them before
persisting.

## When to use this skill

Trigger on any of:

- User wants a mod, trainer, overlay, plugin or runtime layer installed
  on a game they own (Steam / Epic / GOG).
- User has a downloaded archive or DLL and wants it dropped into a
  specific game.
- User wants a registry tweak applied to a single game without touching
  the system global state.
- User wants to *replace* an existing built-in capability with a local
  override (set `overwrite=true` only after explicit confirmation).

Do **not** use this skill for:

- Mods that need new step / check kinds — these require a Rust change in
  the Moddin codebase first; tell the user to open an issue / PR.
- Cross-store content (Microsoft Store, Battle.net, …) — Moddin
  currently discovers Steam + Epic + GOG only.
- Anything that would write to HKLM or require admin rights — Moddin's
  `path_guard` rejects HKLM and the install will fail. Refuse and
  explain.

## Workflow

Follow these steps in order. Do not skip the **validate**, **preview**,
or **confirm** steps.

### 1. Understand intent

Turn the user's free-form request into:

- **target game** (moddin gameId, e.g. `elden-ring`).
- **capability pattern**: `extract-zip` (most mods), `proxy-dll` (single
  DLL under a proxy name), `registry-override` (HKCU tweak only), or
  `vr-runtime-overlay` (OpenXR layer).
- **source** of the mod: a URL the user provides, a local archive the
  user already downloaded, or — if the user has nothing — a search.

If you cannot pin down the game, call `list_supported_games` and ask the
user to pick.

If the user mentions a Nexus / GitHub / Steam Workshop URL you have not
seen before, treat it as the source. **Do not download binaries
yourself** — only inspect the URL, the listed release tag, and any
SHA-256 the upstream publishes. Ask the user to either:

- drop the file in a known folder and give you the path, or
- confirm the URL and the upstream SHA-256 they trust.

### 2. Resolve game + engines

Call `get_game_info({ gameId })`. Note the executable relative path and
the list of engines the game supports (Unreal 5, Redengine, RE Engine,
Unity, …). Use this to decide which `supportedEngines` to declare and to
sanity-check whether the pattern fits the game.

If the game is not in the catalog, **stop**. Tell the user Moddin does
not yet know this game; the rest of the workflow would fail because
Moddin would not know the executable directory.

### 3. Pull a template

Call `get_capability_template({ pattern })`. Read it. It contains the
canonical field names, check ids, and step ordering for that pattern.
Start from it — never invent field names from scratch.

### 4. Collect missing config

The template's `configSchema` lists the fields the user must supply.
Walk the user through them one at a time. The agent may:

- compute the SHA-256 of a local file the user provides (use a shell
  command — `sha256sum` / `Get-FileHash`).
- pull the SHA-256 from a GitHub release page the user pastes.
- leave a field blank **only** if the template marks it optional and
  the user explicitly opts out.

Never fill a field with a value the user did not authorise.

### 5. Compose the YAML

Take the template, fill in `id` (kebab-case, no spaces), `displayName`,
`category`, `status`, `supportedEngines`, the `configSchema` (add fields
the user agreed to), `install` / `uninstall` / `verify`, and the
`safetyNotes` (rewrite in the user's language).

Rules:

- `id` must be unique; call `list_capabilities` first to avoid clashing
  with a built-in.
- The `displayName` is what the user sees in the Moddin UI.
- `category` is one of `vr`, `graphics`, `qol`, `system`. If unsure, ask.
- Every config field referenced in `install` / `uninstall` / `checks`
  must exist in `configSchema`.
- Never use a step or check kind outside the enums returned by
  `get_step_kinds` / `get_check_kinds`. If the mod needs a kind the
  runner does not yet support, **stop** and tell the user.

### 6. Validate

Call `validate_capability_yaml({ yaml })`. If `ok` is false, fix the
errors and call it again. Do not move on with errors open.

### 7. Preview

Call `preview_capability_plan({ yaml })`. Read the rendered plan back to
the user in plain language. Highlight:

- The files the install will create / overwrite.
- Any registry keys it will write.
- Any side-processes it will start.
- The blocker checks that must pass before Apply is enabled.

### 8. Confirm

Ask the user explicitly:

> "I'm about to save `fps-unlocker.yaml` to
> `%LOCALAPPDATA%/Moddin/capabilities/`. Moddin will pick it up on the
> next launch and you can Apply it under Elden Ring. The install will
> touch these files: ... Proceed?"

Never call `save_capability_yaml` without the user saying yes.

### 9. Save

Call `save_capability_yaml({ yaml, overwrite })`. Set `overwrite` only if
the user is replacing an existing local file and explicitly asked for it.

On success, tell the user:

1. Open Moddin Desktop.
2. Click **Rescan stores** in the top bar (or restart Moddin).
3. Pick the target game. The new capability is listed under its category.
4. Click **Apply** — Moddin shows the preview, then installs with
   rollback.

### 10. Hand off

The agent's job is done. Moddin handles the rest: preview → backup →
install → verify → activity log. If the user comes back later to remove
or tweak the mod, they do it from Moddin's UI, not from the agent.

## Style

- Match the user's language (Portuguese in this environment unless they
  switch).
- Never dump the raw YAML into the chat unless the user asks. Show them
  the preview plan and a one-paragraph summary.
- When the user has a partial answer, accept it and ask for the missing
  one. Do not interrogate.
- Cite upstream URLs the user provided verbatim; do not invent.

## Hard limits

These are non-negotiable. Violating any of them makes the agent unsafe
to run.

1. **Never invoke install / uninstall / rollback.** Those go through
   Moddin's UI by the user's hand, with the preview and transaction.
2. **Never write outside `%LOCALAPPDATA%/Moddin/capabilities/`.** The
   MCP server is restricted to this folder by `save_capability_yaml`.
3. **Never bypass `validate_capability_yaml`.** Every save goes through
   it; the tool refuses invalid YAML.
4. **Never auto-fill security-sensitive fields** (SHA-256, downloadUrl,
   registry keys) with values the user did not provide. Empty is OK;
   guessed is not.
5. **Never set `overwrite=true`** without an explicit user instruction
   on this turn. Built-in capabilities are read-only; local files are
   user-authored and should only be replaced on demand.

## Tool reference

| Tool | When to call |
| --- | --- |
| `moddin_status` | Optional. On startup if the agent is unsure the MCP server is wired up. |
| `list_supported_games` | When the user did not name a game, or you need to verify Moddin recognises it. |
| `get_game_info` | After pinning the gameId. |
| `list_capabilities` | Before choosing `id`, to avoid clashes with built-ins. |
| `get_capability` | To inspect an existing capability before extending it. |
| `get_step_kinds` / `get_check_kinds` | Whenever you are unsure a kind is supported. |
| `get_capability_template` | Step 3 — pull the right starter. |
| `validate_capability_yaml` | After composing; before preview; before save. |
| `preview_capability_plan` | To show the user the predicted diff before save. |
| `save_capability_yaml` | The single write. With `overwrite=true` only when the user explicitly asks. |