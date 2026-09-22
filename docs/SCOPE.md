# Moddin Desktop — Project Scope

Moddin Desktop is a small, opinionated Windows app. This document is a one-page summary of what the project **does**, what it **does not do**, and where it is heading. For the deeper architecture and rationale, see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## What Moddin Desktop is

Moddin Desktop is a **declarative package manager for PC game mods and tools**. It detects your installed Steam and Epic games, matches them against a catalog of supported titles, and walks you through installing mod recipes with a full preview + undo support.

The catalog is **declarative**, not code. Adding support for a new game is a single YAML file — no Rust, no Vue, no recompile. The same model powers both built-in recipes and the community catalog at [petonexus/moddin-community-capabilities](https://github.com/petonexus/moddin-community-capabilities).

---

## What you can do with it

- **VR-ready launch profiles** for Elden Ring, Cyberpunk 2077, Dawnwalker, STALKER 2, Dead Island 2
- **OBS VR Capture** — clone a Game Capture source, back it up, target the game
- **OFXR Bridge FrameGen** — download the pinned pre-release, verify SHA-256, configure and start the tray
- **UEVR engine-aware installer** — supports Nightly, JoeyHodge, and PureDark AFW backends
- **OptiScaler** — DLSS upscaler installer with conflict-aware proxy selection
- **Cheeky Foveated DLSS** — ReShade add-on installer
- **OpenXR helpers** — per-game OpenXR runtime override (no admin needed)
- **ReShade host** — full installer with proxy rename
- **Desktop shortcut creator** — `.lnk` next to the game executable

Plus the **community catalog**: data-driven capabilities maintained by the community, signed with Ed25519, automatically visible to every Moddin user.

---

## What Moddin Desktop is not

- **Not cross-platform.** Windows 10/11 x64 only. Steam Deck mode is out of scope.
- **Not an anti-cheat bypass.** EAC / BattlEye / Vanguard must already be disabled by the user's own setup. Moddin never flips that switch.
- **Not a Nexus Mods client.** Moddin is a local manager. It does not host mods.
- **Not a cloud service.** No telemetry, no analytics, no account. The app is 100% local; the only network it does is fetching the community catalog.

---

## How the pieces fit

```
┌─────────────────────────────┐
│ Moddin Desktop (this repo)  │  Windows app (Tauri + Vue)
│                             │
│ • Built-in capabilities     │   https://github.com/petonexus/moddin-desktop
│ • Capability pipeline       │
│ • Community catalog fetcher │
│ • Settings → Community UI   │
└─────────────────────────────┘
                ▲
                │ reads signed catalog.json + .sig
                │
┌─────────────────────────────┐
│ Community Capabilities       │   Catalog + CI
│ (separate repo)             │
│                             │   https://github.com/petonexus/moddin-community-capabilities
│ • capability.yaml per game  │
│ • Signed by maintainer key  │
│ • Validated by Ed25519      │
└─────────────────────────────┘
```

The app ships with the built-ins; the community catalog extends the list.

---

## How "data-driven" works

Every capability is a single YAML file declaring:

1. **Checks** to run before install (game present, archive reachable, hash matches, …)
2. **Install steps** to execute (download, extract, write INI, spawn tray, …)
3. **Uninstall steps** to reverse the install
4. **Safety notes** to display to the user

The app loads the YAML, runs the checks, shows a preview, runs the steps, and records a transaction for Undo. The exact same pipeline serves both built-in recipes and the community catalog — adding a new mod is one YAML file.

For the full schema, see [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md). For the implementation, see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Current status

| Capability | Status |
| --- | --- |
| Steam + Epic install discovery | Stable |
| Game scanning and catalog matching | Stable |
| OBS VR Capture (Elden Ring, Cyberpunk 2077) | Stable |
| OFXR Bridge FrameGen | Stable |
| UEVR (Nightly + Joey + AFW) | Stable |
| OptiScaler | Stable |
| Cheeky Foveated DLSS | Stable |
| ReShade host | Stable |
| OpenXR per-game runtime override | Stable |
| Community catalog (Ed25519-signed) | Stable |
| Local override directory | Stable |
| Transactional Undo | Stable |
| Activity log | Stable |
| Auto-update | Beta |
| 100+ catalog games | In progress |
| Mac / Linux | Out of scope |

---

## Where to read more

- [USER-GUIDE.md](USER-GUIDE.md) — how to install and use Moddin
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to add a capability
- [DEVELOPMENT.md](DEVELOPMENT.md) — how to build Moddin from source
- [ARCHITECTURE.md](ARCHITECTURE.md) — how the code fits together
- [MODULES.md](MODULES.md) — design notes for the future module work
- [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md) — full capability schema
- [ROADMAP.md](../ROADMAP.md) — what is shipping next

---

## License

[GPL-3.0](../LICENSE) — same license as the catalog. One source of truth.
