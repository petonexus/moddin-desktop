# Contributing to Moddin

Thank you for helping build Moddin! There are many ways to contribute, from filing bugs to shipping new mods to writing Rust. Pick what fits you.

---

## Ways to help

| You want to … | Start here |
| --- | --- |
| Add a capability for a game you own | [Adding a capability](#adding-a-capability) |
| Help the community review submissions | [MAINTAINERS.md (community repo)](https://github.com/petonexus/moddin-community-capabilities/blob/main/MAINTAINERS.md) |
| Report a bug or request a feature | [Bug reports](#bug-reports) |
| Improve the docs | [Improving docs](#improving-docs) |
| Translate the app | [Translation](#translation) |
| Code in Rust (Tauri commands, capabilities, etc) | [DEVELOPMENT.md](DEVELOPMENT.md) |
| Code in Vue 3 (UI, components) | [DEVELOPMENT.md](DEVELOPMENT.md) |
| Test pre-release builds and report issues | [Testing beta releases](#testing-beta-releases) |

---

## Adding a capability

Most contributors are adding **capabilities** — recipes that install mod packs, tweaks, or scripts into a game. You don't need to write Rust: a capability is a single YAML file.

### Where the YAML lives

| Repository | When to use |
| --- | --- |
| [petonexus/moddin-desktop](https://github.com/petonexus/moddin-desktop) `src-tauri/capabilities/` | Built-in capabilities — for first-party recipes that ship with the app |
| [petonexus/moddin-community-capabilities](https://github.com/petonexus/moddin-community-capabilities) `capabilities/<id>/capability.yaml` | Community catalog — for everything else |
| `%LOCALAPPDATA%\Moddin\capabilities\<id>.yaml` | Your own private capabilities — drop a file and restart |

### The minimum schema

```yaml
id: community-my-mod
displayName: My Community Mod
category: graphics
status: available
configSchema:
  - name: version
    type: string
    required: true
  - name: downloadUrl
    type: url
    required: true
  - name: sha256
    type: sha256
    required: true
install:
  - kind: extract-zip
    params:
      archivePathField: downloadUrl
safetyNotes:
  - Always close the game before applying.
```

That's the entire capability — a few metadata fields, a list of typed steps, and the safety notes your users will see in the UI.

### The available step kinds

| Kind | What it does |
| --- | --- |
| `extract-zip` | Downloads a ZIP archive and extracts it next to the game executable. Optional proxy rename. |
| `verify-hash` | Compares the SHA-256 of a downloaded file against an expected value. |
| `file-delete` | Deletes a file you specified. |
| `write-text-file` | Writes a text file (e.g. `.ini`) with `{placeholder}` substitution from `configSchema` values. |
| `write-binary-file` | Same as `write-text-file` but for base64 payloads. |
| `move-file` | Renames a file inside the executable directory. |
| `spawn-process` | Starts a process (e.g. `OFXRBridgeTray.exe`). |
| `kill-process` | Stops a process (e.g. `OFXRBridgeTray.exe`) before removing it. |
| `registry-write` | Writes a Windows registry key under HKCU. |
| `registry-delete` | Deletes a Windows registry key under HKCU. |

Unknown kinds fail to load — the catalog loader refuses them. Adding a new kind requires a Rust PR in the app repo.

### The available check kinds

Checks run before and after install to surface what the runner needs:

| Kind | What it evaluates |
| --- | --- |
| `process-running` | Is the named Windows process alive? |
| `file-exists` / `file-absent` | Does the file exist? |
| `archive-reachable` | Does the URL respond? |
| `archive-sha256` | Does the downloaded archive hash match? |

Checks can be `info`, `warning`, or `blocker`. A `blocker` that fails disables the Apply button.

### The full contract

For the complete schema (severity, category, config types, step params, the fail-closed behavior, signing flow), see [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md).

### Submit a capability to the community catalog

1. Fork [petonexus/moddin-community-capabilities](https://github.com/petonexus/moddin-community-capabilities).
2. Add `capabilities/<your-mod>/capability.yaml`.
3. (Recommended) Sign with your Ed25519 key:
   ```bash
   python scripts/sign.py --generate-key-pair   # one-time
   python scripts/sign.py capabilities/<your-mod>/capability.yaml
   ```
   Commit the resulting `SIGNED-BY` (public key + signature). The `*.key` file is **never** committed.
4. Validate locally:
   ```bash
   python scripts/validate_capability.py capabilities/<your-mod>/capability.yaml
   ```
5. Open a PR. CI runs `validate.yml`. A maintainer reviews and merges.
6. Once merged, the `build-catalog.yml` workflow re-signs and publishes. Your capability shows up in Moddin Desktop's **Community** panel within the catalog TTL (default 24h).

---

## Bug reports

A useful bug report includes:

1. **Moddin version** (Settings → About, or `%LOCALAPPDATA%\Moddin\config\version.json`)
2. **Windows version** (Windows 10 22H2, Windows 11 23H2, etc.)
3. **Game + module** (e.g. "Elden Ring + OFXR Bridge FrameGen")
4. **Steps to reproduce** (what you clicked, what you expected, what happened)
5. **Relevant activity log entries** (top-right `≡` button → filter by error → copy)
6. **Game-side logs** if relevant (e.g. `ERVR.log` for UEVR issues)

Open an [issue](https://github.com/petonexus/moddin-desktop/issues) with the **bug** label.

---

## Improving docs

Docs live in `docs/`:

| File | Audience |
| --- | --- |
| `docs/USER-GUIDE.md` | End users (players) |
| `docs/CONTRIBUTING.md` | New contributors (you are here) |
| `docs/DEVELOPMENT.md` | Engineers building Moddin |
| `docs/ARCHITECTURE.md` | Engineers maintaining Moddin |
| `docs/SCOPE.md` | Everyone (the project charter) |
| `docs/MODULES.md` | Engineers maintaining module recipes |
| `docs/CAPABILITY-CONTRACT.md` | Capability authors |

To propose a docs change:

1. Edit the relevant `.md` file in your fork.
2. Open a PR with a clear summary of what you changed and why.
3. A maintainer reviews for clarity and accuracy.

---

## Translation

Moddin's UI strings live in `src/i18n/locales/`:

- `en.ts` — English (source)
- `pt-BR.ts` — Brazilian Portuguese
- `es.ts` — Spanish

To add or improve a translation:

1. Copy the keys from `en.ts` you want to translate.
2. Add them to the other locale files with the translated values.
3. Validate with `npm run build` (vue-tsc catches missing keys).
4. Open a PR.

---

## Testing beta releases

Each pre-release tagged `vX.Y.Z-beta.N` has a **beta** release in the [Releases](https://github.com/petonexus/moddin-desktop/releases) page.

To test:

1. Download the `.msi` from the latest beta release.
2. Install over your current Moddin (your data is preserved).
3. Try the changes listed in the release notes.
4. File any issues with the **beta** label and the version number.

Beta testers get early access and direct contact with the maintainer team via [Discussions](https://github.com/petonexus/moddin-desktop/discussions).

---

## Code of conduct

Moddin is a small project; we don't have a separate code of conduct document. The TL;DR:

- Be kind. Especially to first-time contributors.
- Critique ideas, not people.
- Assume good faith.
- If a maintainer rejects your PR, ask why — there's almost always a good reason, and the answer often becomes a doc improvement.

---

## Other ways to help

- **Star the repo** — helps others find Moddin.
- **Tweet / blog** — if you use Moddin in your workflow, share it.
- **Answer questions** in [Discussions](https://github.com/petonexus/moddin-desktop/discussions).
- **Test games you own** and report which ones work / which need recipes.
- **Help review community capabilities** — even non-maintainers can leave comments on PRs.

Thank you for being here. 🟢