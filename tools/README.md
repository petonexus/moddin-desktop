# `tools/` — Standalone Windows tools

`tools/` contains **standalone Windows-first packages** that can be invoked
directly by the user, independently of the Moddin Desktop app.

A tool is the right home for a workflow that genuinely needs Windows shell
semantics, its own state directory, or a quarantine / restore flow that does
not fit cleanly into the transaction store.

**If the workflow is reusable across games and the app can express it, it
becomes a YAML capability**, not a Rust module and not a tool — see
[`docs/MODULES.md`](../docs/MODULES.md#two-ways-to-add-a-mod). `tools/` is for
what has to run *without* Moddin installed; a capability is for what runs
*through* Moddin, with its preview, transaction and Undo. Rust comes into it
only when the capability engine is missing a step or check kind the workflow
needs — see [`docs/MODULES.md`](../docs/MODULES.md#two-ways-to-add-a-mod).

---

## Layout

Every tool lives in `tools/<topic>/` and **must** ship the following:

```
tools/<topic>/
├── README.md                  # What it does, who it is for, when not to use it
├── MODDIN-AGENT-CONTEXT.md    # Machine-friendly summary the Moddin agent reads
├── <verb>.cmd                 # One or more .cmd entry points the user runs
└── scripts/
    └── <core>.ps1             # PowerShell engine (invoked by the .cmd files)
```

Optional but recommended:

```
└── .gitignore                 # Excludes runtime dirs (backups/, logs/, state/, .cache/)
```

### README.md

Human-readable, in the same language as the rest of the repo (currently
Brazilian Portuguese for Moddin-authored docs). Must include:

- **Purpose** in one paragraph.
- **Usage** — the exact `.cmd` invocations and any arguments they accept.
- **State layout** — which directories the tool creates under the user's
  profile and what they contain.
- **Backup / restore** — how the tool records undo state and how the user
  triggers a restore.
- **What the tool will not do** — explicit non-goals (e.g. "never updates
  OFXR", "never touches anti-cheat").

### MODDIN-AGENT-CONTEXT.md

A structured summary a future Moddin agent can read without parsing the
PowerShell source. Must include:

- Status and version of the documented baseline.
- The known-good state the tool assumes / restores.
- Hardware / runtime context (GPU, headset, OpenXR runtime) the tool is
  calibrated for.
- Components the tool considers part of the baseline.
- Components explicitly **outside** the baseline (with the reason).
- Sources / upstream references with the date they were consulted.
- Automation rules — what Moddin may automate in this preset and what it
  must never automate.

### `<verb>.cmd`

Thin Windows shell shim that:

- Sets `cd /d "%~dp0"` so the tool works regardless of the user's CWD.
- Forwards `-Mode <Verify|Apply|Restore>` (or the tool's verbs) to the
  PowerShell engine.
- Accepts a `-GamePath "<absolute path>"` argument when the tool targets a
  game.
- Surfaces `$LASTEXITCODE` and pauses on user-visible errors.
- **Never** inlines the core logic in `.cmd` itself.

### `scripts/<core>.ps1`

The PowerShell engine. Should:

- Use `Set-StrictMode -Version 2.0` and `$ErrorActionPreference = 'Stop'`.
- Define `Write-Info / Write-Ok / Write-Warn / Write-Err` helpers.
- Quarantine, never delete, files it considers experimental.
- Keep runtime artifacts (backups, logs, state, caches) **outside** the
  Git tree, typically under `%LOCALAPPDATA%\Moddin\<tool>\`.

### `.gitignore`

Exclude runtime directories. Suggested content:

```gitignore
backups/
logs/
state/
.cache/
```

---

## Current tools

| Tool | Purpose | Module overlap |
| --- | --- | --- |
| `elden-ring-ervr-ofxr-baseline/` | Reversible baseline pack for Elden Ring: ERVR + VDXR + OFXR. Applies a known-good `ERVR.ini`, quarantines experimental extras, restores state. | Overlaps with `uevr`, `openxr`, `ofxr` Rust modules; intentionally scoped to inspection, not install/update. |
| `cheeky-foveated-dlss/` | Multi-game scanner + installer for Cheeky Foveated DLSS. READY / EXPERIMENTAL_READY classification. | Overlaps with the Rust `cheeky` module (ReShade add-on path); covers the UEVR-plugin and Cyberpunk VR Port paths the Rust module does not. |

The overlap is **intentional**: tools run without the app, so they must be
self-sufficient even when the user has not installed Moddin Desktop. Where a
flow converges, it converges on a **capability recipe** — the roadmap's goal
is one declarative description of the install, not a second Rust installer
alongside the first.

---

## Adding a new tool

1. Create `tools/<topic>/` with the layout above.
2. Write `MODDIN-AGENT-CONTEXT.md` first — it forces you to define the
   baseline before touching scripts.
3. Implement `scripts/<core>.ps1` and the `.cmd` shims.
4. Add an entry to the table in this file.
5. If the tool is meant to also be available from inside the Moddin Desktop
   app, write a **capability recipe** for the parts of it that the engine can
   already express, and add a step kind for the parts it cannot — not a second
   implementation in Rust. The tool keeps its documented contract; the recipe
   names what it automates.

---

## Anti-patterns

- **Per-game tool directories.** A tool should target a coherent workflow,
  not a single game. If only one game needs it, the workflow probably
  belongs in a per-game catalog recipe.
- **PowerShell that bypasses Moddin's transaction store.** If the tool
  performs a destructive action the Moddin app can also perform, expose
  the same undo metadata so the in-app transaction history can restore it.
- **Tools that reach outside the game install and the user profile.**
  System-wide changes (services, scheduled tasks, global registry beyond
  per-user OpenXR layers) are reserved for the app and require explicit
  UAC.