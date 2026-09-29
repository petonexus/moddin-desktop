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
| Know how to name a branch or write a commit | [Conventions](#conventions) |

---

## Conventions

These are not aspirations. Every one of them was read off the existing history
(`git log`, and the branch refs in `.git/refs/heads` and `.git/packed-refs`)
before being written down. If you find yourself breaking one, change this
document in the same commit.

### Branch names

`<type>/<short-kebab-description>`, one branch per piece of work:

| Prefix | For |
| --- | --- |
| `feat/` | A new capability, module, command or user-facing feature. The most common prefix here. |
| `fix/` | A defect, on any layer. `fix/capability-engine-integrity` — the branch that carried the 2026-09-29 capability-engine fix pass — is one. |
| `refactor/` | Restructuring with no behaviour change — 18 branches, mostly the `App.vue` decomposition. |
| `hardening/` | A change that makes something fail safer rather than make it work: bounds, locking, snapshotting, recovery, refusal paths. |
| `docs/` | Documentation only. |
| `chore/` | Repo mechanics that are not features, fixes or refactors: release prep, dependency bumps, removing dead files. |

`feat/` (7 branches) and `refactor/` (18) are attested many times over,
`hardening/` seven times, `docs/` and `fix/` once each. `chore/` is the repo's
commit type for exactly this kind of work and is the right branch prefix for it,
but no branch has ever carried it — a gap in the practice, not in the rule. Two
other prefixes exist historically, `reliability/` and `backup/` (one branch
each); neither is current practice.

**CI only runs on `main`, `feat/**` and `refactor/**` pushes**
(`.github/workflows/ci.yml`). A `fix/`, `hardening/`, `docs/` or `chore/` branch
gets no feedback until you open a PR — which is the gate for those anyway.

### Commits

[Conventional Commits](https://www.conventionalcommits.org/): `type(scope): subject`,
subject in the imperative, lower case, no trailing period.

Seven types are in use. The distribution over the last 37 non-merge commits:

| Type | Count | For |
| --- | --- | --- |
| `feat` | 11 | A new capability, module, command, or user-facing behaviour. |
| `fix` | 10 | A defect, or a promise the code does not keep. |
| `chore` | 8 | Release prep, dependency work, removing dead code and branches. |
| `docs` | 3 | Documentation only. |
| `ci` | 3 | Workflows and the guards that gate them. |
| `test` | 1 | Tests added or fixed, with no production change. |
| `release` | 1 | The one commit that changed the shipped product line (MSI → NSIS). |

A `release:` commit is a shipped-artifact change, not a routine chore. If you
find yourself writing one, that is a decision, not a chore.

The scope is the area, not the type: `catalog`, `capability`, `ai`, `runtime`,
`docs`, `frontend`, `trust`, `rust`, `release`, `activity`, `openxr`,
`community`, `ui` are all in use. Two adjacent ids — `capability` and
`capabilities` — both appear; pick the one matching the file you touched and do
not add a third spelling for the same area.

### One roadmap story per commit

A **story** is one reviewable outcome. A commit may close several closely
related audit ids, but it must not mix stories. `1818fdd` closes P0-1, P0-2,
P0-3 and F-02 in one pass because all four are the capability engine failing to
install what it advertises; that is one story. A commit that fixes a YAML typo
and changes a registry default is two.

Every `ROADMAP.md` item is meant to carry a **Done when** line, and the roadmap
is explicit that an item is not done until that line is true and reviewable.
When a commit satisfies one, the body says which item and names the evidence —
usually the test that now enforces it
(`an_install_backs_up_an_existing_game_file_before_overwriting_it`) or the guard
that now fails without it.

### What a commit body owes the reviewer

The diff shows what changed. The body has to carry what the diff cannot:

1. **The id it closes** — the audit or roadmap item, at the top. `P0-1:`,
   `O-06:`, `F-05:`. Without this, nobody can tell whether the item is finished.
2. **The failure, concretely.** The mechanism, not the symptom: *"extract-zip
   resolves its archive with `resolve_download_path`, which only
   special-cases a bare filename"* — not *"extract-zip was broken"*.
3. **The evidence.** Which test now covers it, and how the fix was verified. If
   a guard was added, say how you proved it fires: *"verified by deliberately
   removing `download-file` from the Python set: the guard failed, and passed
   again once restored."*
4. **What you deliberately did not do, and why.** `1818fdd` leaves
   `ofxr-bridge` and `uevr` as `status: planned` and says which step kind each
   one is waiting for. A reviewer needs to know a scope was left open on
   purpose.
5. **Anything a human has to do by hand**, and anything that stays uncovered.
   *"None of this is covered by a test: the project still has no frontend test
   runner, which is F-12."*

A body that only restates the subject line is worse than an empty one — it
costs the reviewer time and tells them nothing.

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
  - kind: download-file
    params:
      urlField: downloadUrl
      expectedField: sha256
      target: my-mod.zip
  - kind: extract-zip
    params:
      archivePath: my-mod.zip
safetyNotes:
  - Always close the game before applying.
```

That's the entire capability — a few metadata fields, a list of typed steps, and the safety notes your users will see in the UI.

### The available step kinds

| Kind | What it does |
| --- | --- |
| `download-file` | Fetches an HTTPS URL into Moddin's download cache, optionally verifying SHA-256. Name the target with a bare filename so the archive stays out of the game folder. |
| `extract-zip` | Extracts a ZIP archive into the game executable directory. Optional proxy rename. |
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
| `exe-version` | Is the game exe's FileVersion inside the supported window? |

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

Docs live in [`docs/`](README.md), which indexes every one of them and says what
each is for. If you add a document, add the row there too.

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

1. Download the `Moddin.Desktop_*_x64-setup.exe` from the latest beta release.
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