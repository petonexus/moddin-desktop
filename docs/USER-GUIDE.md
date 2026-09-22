# Moddin Desktop — User Guide

This guide assumes you have already [installed Moddin Desktop](https://github.com/petonexus/moddin-desktop/releases). It explains how to use every feature in plain language.

---

## 1. First launch

When you open Moddin Desktop for the first time:

1. The app scans your **Steam** and **Epic Games Launcher** libraries.
2. Each detected game appears in the left sidebar under **Library**.
3. Games that Moddin knows about show a green badge; games it doesn't know about show a yellow badge.

You can re-scan at any time by clicking **Rescan stores** at the top of the library.

> 💡 If a game doesn't show up, click **Rescan stores**. Steam and Epic can take a few seconds to release their library files.

---

## 2. Installing a mod on a game

1. **Click a game** in the library to open its detail view.
2. The detail view shows the available mod recipes for that game, organised by category:
   - **VR** — VR-related mods (OBS VR, UEVR, OFXR, OpenXR)
   - **Graphics** — performance and visual mods (OptiScaler, Cheeky DLSS, ReShade)
   - **QoL** — quality-of-life tools (desktop shortcuts, VR launch profiles)
   - **System** — system-level tweaks
3. Each module card shows:
   - A **status badge** (Ready / Installed / Attention)
   - A **verification checklist** of prerequisites (game present, game not running, archive reachable, etc.)
   - **Apply / Remove / Reinstall** buttons
4. Click **Apply**. Moddin shows a preview of every file it will touch, every registry key, and the new install transaction.
5. Click **Confirm**. Moddin:
   - Backs up the current state to a transaction
   - Downloads what it needs (when applicable)
   - Applies the changes
   - Records the install in the activity log
6. To **undo**, click the module's **Remove** button (or **Undo** in the activity log). Moddin restores the backed-up state.

> 🔁 Moddin is **transactional**. Every install has an Undo. Even weeks later, you can roll back.

---

## 3. The activity log

Every Moddin action (install, remove, launch, signature check, error) is recorded in the activity log. To open it:

1. Click the **≡** button at the top of the app.
2. Filter by level (success / warning / error / info) or search by keyword.
3. Click **Details** on any entry to see the file paths, transaction ID, and related game.

The activity log is stored in your user profile and rotates automatically when it gets large.

---

## 4. Updating Moddin

Moddin checks GitHub Releases for new versions. To update:

1. Wait for the in-app update banner (it appears when a new release is published).
2. Click **Download update**.
3. Restart Moddin to apply the patch.

You can also update manually by downloading the latest `.msi` from [Releases](https://github.com/petonexus/moddin-desktop/releases).

---

## 5. Community catalog

The **community catalog** is a curated list of capabilities maintained by the Moddin community at [petonexus/moddin-community-capabilities](https://github.com/petonexus/moddin-community-capabilities). It extends the built-in catalog with more games, presets, and experimental features.

### Opening the Community panel

Click the **⇆** button at the top of the app. The Community panel opens with:

- The list of community capabilities available for your installed games
- A **Refresh** button (fetches the latest catalog)
- A **TTL select** (how often the cache refreshes: 1h / 6h / 24h / 7d / manual)
- The **pinned public key fingerprint** so you can verify what key the catalog was signed with

### Refreshing the catalog

1. Click **Refresh now** to fetch the latest signed catalog from GitHub.
2. Moddin verifies the Ed25519 signature against the pinned public key.
3. If verification fails, the panel shows the error and falls back to the last good cache.
4. If verification succeeds, the panel shows the updated list.

> 🔐 The signature protects you from network tampering. Moddin **never** installs a community capability whose catalog fails signature verification.

### Installing a community capability

1. Pick a game in the library first (the Community panel reads the selected game's config).
2. Open the Community panel.
3. Find the capability you want.
4. Click **Install**.
5. If the capability is **unsigned** (no `SIGNED-BY` in the repo), Moddin asks you to confirm:
   > This capability is unsigned. Install anyway?
6. Moddin downloads the YAML, verifies it, and installs.

### Trust levels

| Origin | UI badge | Install prompt |
| --- | --- | --- |
| **Built-in** | Verified | None |
| **Community + signed** | Verified | None |
| **Community + unsigned** | Unverified | Confirmation required |

You can configure the TTL in the panel's **Refresh every** select.

---

## 6. Adding your own capabilities

If you maintain a private mod pack or just want a custom config, drop a YAML file into:

```
%LOCALAPPDATA%\Moddin\capabilities\<your-capability-id>.yaml
```

Moddin scans this folder at startup and overlays any matching capabilities over the built-ins. See [CONTRIBUTING.md](CONTRIBUTING.md) for the YAML schema.

> 💡 Custom local capabilities get the **Local** badge in the UI. They use the same kind allow-list as built-ins, so unsafe step kinds are rejected at load time.

---

## 7. Troubleshooting

### "Game executable not found"

- The game isn't installed in a standard location. Check **Settings → Library paths** (TODO).
- Verify the catalog recipe points to the right executable. See [CONTRIBUTING.md](CONTRIBUTING.md) for the schema.

### "Game is running — close it before applying"

Some mods (UEVR, OFXR) require the game to be closed before they can rewrite proxy DLLs or registry keys. Moddin refuses to apply them while the game is running. Close the game and click **Apply** again.

### "Archive signature mismatch"

The download URL or hash in the catalog is stale. Open a PR against the community repo or update your local catalog by clicking **Refresh now**.

### "Catalog signature invalid"

The pinned public key in the app doesn't match the one in the catalog. This usually means:
- The maintainer rotated their signing key and a new Moddin release is required
- A network attacker is injecting a fake catalog

**Do not install anything** until you confirm which it is. Open an issue on the Moddin Desktop repo with the catalog fingerprint from the panel header.

### "OFXR Bridge tray not running"

OFXR Bridge requires the tray (`OFXRBridgeTray.exe`) to be running for the layer to activate. Moddin starts it automatically after install; if it's not running:
1. Check the Windows notification area for the OFXR Bridge icon.
2. Right-click → **Open** to see status.
3. If the icon is missing, run `OFXRBridgeTray.exe` manually from `%LOCALAPPDATA%\Moddin\tools\ofxr-bridge\`.

---

## 8. Where data is stored

Everything Moddin writes lives under `%LOCALAPPDATA%\Moddin\`:

```
Moddin/
├── transactions/            ← backup transactions (undo data)
│   └── <transaction-id>/
│       ├── manifest.json    ← module, game, timestamp
│       └── files/           ← backed-up original files
├── logs/
│   └── actions.jsonl        ← activity log (rotates at 5 MB)
├── community-capabilities/  ← cached community catalog
│   ├── catalog.json
│   ├── catalog.json.sig      ← Ed25519 signature
│   └── pinned-public-key.bin ← pinned bootstrap key
└── tools/
    ├── uevr/<backend>/       ← UEVR backends (Nightly, Joey, AFW)
    ├── optiscaler/
    ├── reshade/
    ├── ofxr-bridge/
    └── cheeky-foveated-dlss/
```

You can delete any of these folders to force a re-install of that tool.

---

## 9. Privacy

Moddin does not collect telemetry, send analytics, or phone home. The app:

- Reads Steam and Epic libraries locally
- Verifies community catalogs against GitHub's public `raw.githubusercontent.com` URLs
- Writes everything under your user profile
- Stores the activity log in a JSONL file you can inspect or delete

To remove Moddin completely:
1. Uninstall from **Settings → Apps → Installed apps**.
2. Delete `%LOCALAPPDATA%\Moddin\`.

No other folders are touched.

---

## 10. Need help?

- **Bug?** [Open an issue](https://github.com/petonexus/moddin-desktop/issues) with:
  - Windows version
  - Moddin version (Settings → About)
  - Steps to reproduce
  - Relevant entries from the activity log
- **Question?** Open a [Discussion](https://github.com/petonexus/moddin-desktop/discussions).
- **Chat?** Join the [Petonexus Discord](#) (TODO: link).
