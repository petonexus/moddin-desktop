# Moddin Desktop

**Manage the mods that power your games — VR mods, performance mods, overlays, and tweaks — from one Windows app that knows how to undo what it did.**

Moddin Desktop detects your installed Steam and Epic games, matches them against a catalog of supported titles, and walks you through installing the right mods. Every change is previewed, backed up, and reversible. No more sticky notes with "what did I install last time?" — Moddin remembers and undoes it for you.

> 🔎 **New here?** Start with the [User Guide](docs/USER-GUIDE.md). It assumes nothing about your background.
>
> 🛠️ **Want to write a capability?** Read [CONTRIBUTING.md](docs/CONTRIBUTING.md) and look at the [community catalog](https://github.com/petonexus/moddin-community-capabilities).
>
> 🏗️ **Building Moddin itself?** See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for setup, tests, and the catalog schema.

---

## What you can do with Moddin today

- **Detect** every Steam and Epic game installed on your PC.
- **Apply** mod recipes (OBS VR capture, OptiScaler, OFXR Bridge, UEVR, ReShade, Cheeky DLSS, OpenXR helpers, desktop shortcuts, …) to the games you own.
- **Preview** every change before it touches your files. Moddin shows the exact files and registry keys it will touch.
- **Undo** any change. Moddin remembers the original state and can roll it back — even weeks later.
- **Install community capabilities** — a verified catalog of recipes maintained by the Moddin community. Verified by Ed25519 signature pinned in the app binary.
- **Track** every install / uninstall / rollback in a structured activity log.

Moddin is **Windows-first** and supports the games listed below out of the box:

| Game | What's available |
| --- | --- |
| Elden Ring | OBS VR Capture, OFXR Bridge FrameGen, UEVR engine-aware installer, OptiScaler, Cheeky DLSS, OpenXR helpers, VR-ready launch profile |
| Cyberpunk 2077 | OBS VR Capture, OptiScaler, Cheeky DLSS, OpenXR helpers, VR-ready launch profile |
| Dawnwalker | All Unreal5-default modules (UEVR, OFXR, OBS VR, OptiScaler, Cheeky DLSS, ReShade, UE4SS) |
| STALKER 2 | All Unreal5-default modules |
| Dead Island 2 | OBS VR, OptiScaler, OFXR, OpenXR helpers |
| DOOM 2016 | Desktop shortcut |

You can also drop your own YAML capability into `%LOCALAPPDATA%\Moddin\capabilities\` — see [USER-GUIDE.md](docs/USER-GUIDE.md#adding-your-own-capabilities).

---

## Getting Moddin

1. **Download** the latest release from [Releases](https://github.com/petonexus/moddin-desktop/releases). Beta releases are tagged `vX.Y.Z-beta.N`.
2. **Install** by running the `.msi` (Windows 10/11 x64).
3. **Open** the app. Moddin scans your Steam + Epic libraries automatically.
4. **Pick a game**, see which mods apply, and install what you need.

### System requirements

- Windows 10 or 11 (x64)
- ~200 MB free disk space for the app + caches
- .NET runtime is **not** required — Moddin ships its own (Tauri + WebView2)
- No admin rights needed for most installs; the app uses per-user paths and HKCU registry keys

---

## Try the community catalog

Moddin ships with a set of built-in capabilities for the most popular games. The **community catalog** adds more, contributed by other users and verified by the Moddin maintainers.

1. Click the **Community** button in the top dock.
2. Press **Refresh now** to fetch the latest catalog.
3. Browse the list — every entry shows whether the author signed their YAML.
4. Click **Install** on any capability to add it to your game.

The fingerprint of the pinned signing key is shown in the panel header so you can verify the catalog is legitimate. See [USER-GUIDE.md → Community catalog](docs/USER-GUIDE.md#community-catalog) for details.

---

## Where to go next

| You want to … | Read this |
| --- | --- |
| Install Moddin and use it | [docs/USER-GUIDE.md](docs/USER-GUIDE.md) |
| Add a capability to a game you maintain | [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) |
| Build Moddin from source | [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) |
| Understand the architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Help the project | [CONTRIBUTING.md → Other ways to help](docs/CONTRIBUTING.md#other-ways-to-help) |
| Report a bug or request a feature | [GitHub Issues](https://github.com/petonexus/moddin-desktop/issues) |
| Stay updated | [Releases](https://github.com/petonexus/moddin-desktop/releases) · [Roadmap](ROADMAP.md) |

---

## Safety and trust

Moddin never asks for admin rights and never disables your game anti-cheat. Every action runs in your user account, writes only to the game directory or per-user registry paths, and is recorded in an activity log so you can audit what changed.

For community capabilities, the catalog is signed with Ed25519 by the maintainers. Moddin verifies the signature against a public key **pinned in the app binary** before installing anything. Unsigned capabilities are accepted only when you tick a confirmation box in the UI.

See [SECURITY.md](docs/CAPABILITY-CONTRACT.md#threat-model) for the full threat model.

---

## License

[GPL-3.0](LICENSE) — the same copyleft license as the community catalog. Moddin and the catalog stay a single source of truth.
