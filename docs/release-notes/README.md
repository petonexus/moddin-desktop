# Release notes

Moddin's published release notes, one file per tag. **The current version is
`v0.1.0-beta.6`** — the newest file below is the current one.

Each file is the body of its GitHub release: the first line is the title, and CI
appends the installer digest it computed from the artifact it actually built.
Files are named after the tag they belong to.

| Version | Headline |
| --- | --- |
| [v0.1.0-beta.6](.release-notes-v0.1.0-beta.6.md) **— current** | The installer finally contains the frontend (it shipped blank before), the first updater-verifiable release, a pinned community keyring, and a stricter install engine |
| [v0.1.0-beta.5](.release-notes-v0.1.0-beta.5.md) | Dependencies between mods, game-build compatibility gating, proxy conflicts that name their owner, the first working `download-file` step, BepInEx / UE4SS / REFramework as recipes, and the Codex `exec` fix |
| [v0.1.0-beta.4](.release-notes-v0.1.0-beta.4.md) | The user-facing word "capability" is gone; the app says **mod** |
| [v0.1.0-beta.3](.release-notes-v0.1.0-beta.3.md) | Redesigned AI surface, errors that say what to do, a smaller per-user installer, community signing-key refresh |
| [v0.1.0-beta.2](.release-notes-v0.1.0-beta.2.md) | Redesigned interface, no console flashes, two Windows fixes |
| [v0.1.0-beta.1](.release-notes-v0.1.0-beta.1.md) | First public beta: preview, backup transaction, Undo |

## Publishing a new one

1. Write `.release-notes-<tag>.md` in this directory, starting with a single
   `#` heading — the publish job turns that line into the release title.
2. Commit it **with** the code it describes. A tag whose notes file is missing
   fails the release job on purpose.
3. `.github/workflows/release.yml` reads `docs/release-notes/.release-notes-${TAG}.md`,
   builds the NSIS installer itself, hashes that artifact, and appends the
   SHA-256 to the body. Do not hand-write a digest: the CI-computed one is the
   only one that matches the file users download.
4. Add a row to the table above.

## Known gaps carried by the current release

Recorded here so the notes are not the only place they exist: the shipped
OptiScaler, ReShade, UEVR, OFXR Bridge and Cheeky recipes passed a URL where a
local path was expected; the `ofxr-bridge`, `uevr`, `ue4ss` and `reframework`
recipes are `status: planned`; and the ReShade host module is not offered by any
catalog engine preset. `ROADMAP.md` tracks each one against its audit id.
