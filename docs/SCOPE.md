# Moddin Desktop — Scope

This document defines **what is and isn't** inside the Moddin Desktop project,
and where each piece of work belongs. It is the source of truth for new
contributions and refactors; if reality drifts from this document, update it
in the same change.

Moddin Desktop is **Windows-first**. Cross-platform support is explicitly out
of scope for the foreseeable future.

---

## 1. The app

`Moddin Desktop` is a small **declarative package manager for PC game mods
and tooling**. It is not a collection of per-game scripts.

### What the app does

- Discovers installed Steam and Epic games (Windows Registry, `.item`
  manifests, multi-library Steam installs).
- Matches each installed game against a declarative catalog.
- Shows which reusable modules are supported for each game.
- Previews the planned changes before applying anything.
- Records a transaction before any mutation so it can be undone.
- Keeps a structured per-action history (separate from in-memory diagnostics).
- Launches the game through a catalog-declared VR / graphics profile when the
  user wants to.

### What the app is not

- It is **not** a launcher. It launches games only as part of a profile the
  user explicitly configured, never as the default play loop.
- It is **not** an anti-cheat bypass tool. Mods that require EAC/BE to be
  disabled must already be disabled by the user's own setup. Moddin never
  flips that switch on the user's behalf.
- It is **not** a generic script runner. Game-specific PowerShell does not
  belong in the app — it belongs in `tools/`.
- It is **not** a catalog provider for third parties. The catalog ships with
  the app and is the source of truth for what Moddin can do locally.

---

## 2. The three layers

Every change must live in exactly one layer.

### 2.1 Catalog (`src/catalog/games/*.yaml`)

Stable game identity (Steam App ID, Epic App Name, executable path) plus the
list of enabled modules and small per-module overrides. Auto-discovered at
build time through `import.meta.glob` — adding a new game **must not** require
registering a new TypeScript import. The catalog schema is validated by Zod
in `src/services/catalog.ts`.

If something is **game-specific**, it goes here as declarative config.

### 2.2 Reusable module (Rust, `src-tauri/src/<module>.rs`)

A module describes **how a capability is applied**. Each native module owns
its download, verification, install, verification pass, and rollback through
the shared transaction store.

If a capability is **reusable across games** and benefits from native code
(registry, filesystem, process, hash, archive), it goes here.

Today: `obs`, `optiscaler`, `openxr`, `ofxr`, `uevr`, `cheeky`, `vr_launch`,
`desktop_shortcut`.

Planned per `ROADMAP.md`: `reshade`, `ue4ss`, `bepinex`, `reframework`.

### 2.3 Tool (`tools/<topic>/`)

A tool is a **standalone Windows-first package** that runs **without the
app**. It uses `.cmd` shims + a `scripts/<core>.ps1` engine and may be
invoked by the user directly, by another tool, or by the app in a future
integration.

If a workflow is best expressed as a Windows script with its own state
directory, backups, and restore flow — and is not yet a good fit for a
reusable Rust module — it goes here.

Today: `elden-ring-ervr-ofxr-baseline/`, `cheeky-foveated-dlss/`.

---

## 3. Decision rules

Use these to decide where new work goes.

### "I want to support a new game"

1. Add `src/catalog/games/<id>.yaml` with verified `id`, `name`, store App
   ID, executable path, and the modules to enable.
2. If a module's `config` cannot express what you need, escalate to a real
   module gap (see below). Do **not** add `if gameId === ...` branches.
3. Update `references/game-support-matrix.md` with the verified identity.

### "I want to add a new capability for several games"

1. Check if a Rust module already covers it with a new `config` key.
2. If not, design a new reusable module: `src-tauri/src/<capability>.rs`,
   exposed through `lib.rs` commands, with preview / apply / verify /
   rollback following the transaction model.
3. Add the corresponding entry to the catalog schema in
   `src/types/game.ts` and the Zod validator in `src/services/catalog.ts`.
4. Only then add the YAML entries for the games that need it.

### "I want a Windows script with its own state directory"

Put it under `tools/<topic>/` with the layout described in
[`tools/README.md`](../tools/README.md). The tool must document:

- what it does and does **not** do;
- what files it touches;
- how it backs up and restores state;
- any preconditions (admin rights, runtime installed, etc.).

If a tool's logic is small and self-contained, consider promoting it into a
Rust module instead. Tools exist for workflows that genuinely need Windows
shell semantics, quarantine directories, or one-off state that does not
belong in the transaction store.

### "I want to add a new VR / graphics primitive"

Same rule as a new capability: declarative config first, native module
second, and only escalate to a `tool` if Windows shell semantics make a
module the wrong shape.

---

## 4. Engine → typical modules

The catalog today does **not** inherit modules from an engine preset. The
table below describes the canonical mapping a future preset layer should
respect; until then, copy the right modules per game YAML.

| Engine | Typical module set |
| --- | --- |
| Unreal Engine 4.8–5.4 | `uevr` (if VR), `obs-vr`, `ofxr-framegen`, `openxr`, `optiscaler`, `cheeky-foveated-dlss` |
| REDengine (Cyberpunk) | `obs-vr`, `openxr`, `optiscaler`, `cheeky-foveated-dlss`; Cyberpunk VR Port owns its own first-launch settings |
| RE Engine (Resident Evil / Dead Island) | `obs-vr`, `uevr` is **not** applicable, `openxr` is engine-specific |
| Unity | `optiscaler`, `cheeky-foveated-dlss`, mod loaders per game |
| id Tech (DOOM 2016) | `desktop-shortcut`; external VR launchers (KHARVOX) are managed outside Moddin until a managed launcher capability exists |

---

## 5. VR / DLSS roadmap coverage

VR-related tooling is treated as **first-class** in the catalog. The
explicit VR-relevant capabilities and where they live:

| Capability | Where |
| --- | --- |
| OBS VR capture | Rust module `obs` + catalog `obs-vr` |
| OpenXR helpers, per-game runtime override | Rust module `openxr` + feature UI |
| OFXR Bridge FrameGen | Rust module `ofxr` + catalog `ofxr-framegen` |
| UEVR engine-aware installer | Rust module `uevr` + catalog `uevr` |
| VR-ready launch profile | Rust module `vr_launch` + catalog `vr-launch` |
| ERVR + VDXR + OFXR baseline (Elden Ring, reversible) | `tools/elden-ring-ervr-ofxr-baseline/` |
| Cheeky Foveated DLSS (ReShade + UEVR paths) | Rust module `cheeky` + `tools/cheeky-foveated-dlss/` |
| DLSS SR via OptiScaler | Rust module `optiscaler` + catalog `optiscaler` |
| DLSS FG / RR (planned unified module) | not yet; tracked in `ROADMAP.md` under v0.4 |
| ReShade full add-on host (planned) | tracked in `ROADMAP.md` |
| UE4SS, BepInEx, REFramework (planned) | tracked in `ROADMAP.md` |

DLSS itself is **not** a single product in Moddin's vocabulary. Moddin covers
DLSS-SR through `optiscaler` (when the game lacks native DLSS), DLSS-SR
natively when the game ships with `nvngx_dlss.dll`, and DLSS-FG / DLSS-RR
through upstream runtime layers (OFXR / ReShade add-ons / Streamline).
Per-game recipes state which path is in use; a future unified "DLSS" module
must not duplicate that.

---

## 6. Repository layout

```
moddin-desktop/
├── app/                  # Tauri + Vue + Rust source (the desktop app)
│   ├── src/              # Vue + TypeScript UI, catalog loader, i18n
│   ├── src-tauri/        # Rust modules, capabilities, config
│   └── src-tauri/gen/    # Generated Tauri schemas (do not edit)
├── tools/                # Standalone Windows tools (run without the app)
│   ├── README.md
│   └── <topic>/          # Per-tool: README + MODDIN-AGENT-CONTEXT + .cmd + scripts/
├── docs/                 # Cross-cutting documentation
│   ├── ARCHITECTURE.md
│   ├── DEVELOPMENT.md
│   ├── FRONTEND.md
│   ├── SCOPE.md          # this file
│   └── ...
├── references/           # Stable reference tables (game-support-matrix)
├── scripts/              # Repository-level helper scripts (setup, lint)
├── ROADMAP.md            # What ships next
├── README.md             # Entry point
├── package.json
├── Cargo.toml            # exists at root only if a workspace is added
└── src-tauri/Cargo.toml
```

The `app/` prefix is a future reorg. Today the app lives at the repo root
(`src/`, `src-tauri/`); this scope document records the target layout so
each refactor step is incremental.

---

## 7. Out of scope

- **Linux/macOS support.** Steam Deck mode is not a target.
- **Anti-cheat bypass.** Moddin will never bundle, recommend, or automate
  disabling EAC, BattlEye, Vanguard, or similar.
- **Mod hosting.** Moddin is not a Nexus/Thunderstore client.
- **Cloud profiles.** No remote account, no telemetry.
- **Auto-update of the app itself** beyond the existing GitHub Releases
  updater (`src-tauri/src/updates.rs`). Hot-reload of recipes from the
  network is **not** in scope; the catalog ships with the binary.