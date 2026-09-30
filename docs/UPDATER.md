# App self-updater

Status: **wired, waiting on one key the maintainer holds.** Everything below
the key is built and tested; nothing below can install an update, by design,
until the key is substituted.

This is ROADMAP `F-04` / `P0-4`. The reason it gets a document instead of a
pull request that "just adds the plugin" is the record in
[AUDIT-2026-09-29.md](AUDIT-2026-09-29.md): the last time the updater was
needed, the signing key was a Rust constant, rotating it meant shipping a
binary, and *"users downloaded a knowingly broken installer"*. An updater
that repeats that trust mistake is worse than no updater, so read the trust
section before changing anything here.

---

## What is already in the tree

| Piece | Where |
| --- | --- |
| Endpoint, pubkey slot, Windows install mode | `src-tauri/tauri.conf.json` → `plugins.updater` |
| Updater bundle artifacts (`.nsis.zip` + `.sig`) | `src-tauri/tauri.conf.json` → `bundle.createUpdaterArtifacts` |
| Plugin registration, commands | `src-tauri/src/app_update.rs`, `src-tauri/src/lib.rs` |
| The commands the UI can call | `app_update_status`, `app_update_check`, `app_update_install` |
| The panel | `src/features/app-update/`, mounted in `src/components/shell/GlobalTools.vue` |
| What the webview is **not** allowed to do | `src-tauri/capabilities/app-update.json` |
| Dependency | `src-tauri/Cargo.toml` → `tauri-plugin-updater = "~2.11.0"` |

---

## Trust

### The key is configuration, and the placeholder is not a key

`plugins.updater.pubkey` currently reads:

```json
"pubkey": "REPLACE_WITH_TAURI_UPDATER_PUBKEY__run `npm run tauri signer generate` and paste the printed `minisign+PUBLIC KEY ...` line here (see docs/UPDATER.md) — every build refuses this value"
```

Three refusals keep that string from behaving like a key:

1. **A release build does not compile with it.**
   `src/app_update.rs` has a `const _: () = assert!(!placeholder_in_place(…))`
   under `#[cfg(not(debug_assertions))]`. `tauri build` — the only thing that
   can produce an installer — fails with a message naming the substitution.
   It is `not(debug_assertions)` on purpose: the repository is *in* this
   state deliberately, and a guard that makes `cargo test` and `cargo clippy`
   permanently red is a guard people delete.
2. **The commands refuse at runtime.** `app_update_check` answers
   `not-configured` and `app_update_install` returns an error, before any
   network call.
3. **There is no fallback.** No arm anywhere in `app_update.rs` proceeds
   without a verified signature. A failure is an error the panel renders
   through `useFriendlyError` + `ErrorCallout`; the installed version is
   what the user keeps in every failure case.

### What the maintainer substitutes, and where

| # | Where | What |
| --- | --- | --- |
| 1 | `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` | The public key printed by `npm run tauri signer generate` (the whole `minisign+PUBLIC KEY …` line). This is the only place a key lives. |
| 2 | GitHub repository secret `TAURI_SIGNING_PRIVATE_KEY` | The private key file, same command. **Never** in a file, a commit or a chat. |
| 3 | GitHub repository secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The password given to `tauri signer generate`, if one was set. |
| 4 | `.github/workflows/release.yml` | Upload the updater artifacts and the manifest. See below. |

Steps 1–3 are the key. Step 4 is what makes the key mean something: a
signature over an artifact nobody publishes verifies nothing.

**Rotation** is the point of doing it this way: swap the private key in the
secret, publish a release signed by it, then substitute the new public key
in `tauri.conf.json` and ship. The public key is in a config file a normal
release edits, so rotation no longer requires an emergency binary — which is
exactly what the audit says went wrong last time. (The community
capability keyring in `community_catalog.rs` is a separate trust anchor with
a different cadence. Do **not** reuse one key for both: a key that signs
installer updates should not also sign a catalog that can name what gets
installed.)

### What is deliberately not verified by this build

* That the substituted key is the *right* key. A syntactically valid but
  wrong key compiles, and every real update then fails as a signature
  refusal. Nothing in the build can catch that; try the first dry-run tag
  (below) and read the panel's answer.
* That a maintainer did not paste the community catalog key in by mistake.
  The keyring is a private const in a file this change did not own, and
  duplicating it here would have created a second list to drift.

### The digest is not ours

`release.yml` builds the installer and appends a SHA-256 computed from that
artifact. This feature neither compares nor stores a digest, and there is
deliberately no second one. Artifact authenticity is the minisign signature;
the SHA-256 in the release body is a human-checkable fact about the same
file, published by the job that produced it.

---

## The release side (owned by whoever maintains `.github/workflows/`)

The endpoint is

```text
https://github.com/petonexus/moddin-desktop/releases/latest/download/latest.json
```

so a release has to publish three things:

1. `<name>_<version>_x64-setup.nsis.zip` — produced by
   `bundle.createUpdaterArtifacts: true`;
2. `<name>_<version>_x64-setup.nsis.zip.sig` — the minisign signature, from
   the signing key above;
3. `latest.json` — the manifest, with the `windows-x86_64` entry's `url`
   pointing at the asset URL of (1) and its `signature` holding the
   contents of (2).

`latest.json` has to be **generated in CI from the signed artifact**, not
typed. `tauri-action` does this for a Tauri release automatically; a
hand-written manifest is the same class of mistake as a hand-typed digest.

**Stable channel only, explicitly.** `releases/latest/…` never resolves to a
draft and never to a prerelease — GitHub's `latest` excludes both, and
`release.yml` marks any tag containing `-` as `--prerelease`. The Rust side
enforces the same rule independently (`decide` refuses any version with a
prerelease suffix), so a replaced or misconfigured manifest cannot talk a
stable install into a beta. There is no beta channel: the panel says
`stable`, and if you add one, add it as a separate endpoint, not as a
fallback inside the stable check.

---

## What a user sees, and what a failure leaves behind

Open **Help and diagnostics → Update Moddin**. Nothing checks for an update
before that, and there is no timer anywhere in the feature.

| Step | Behaviour |
| --- | --- |
| Open | Reads this build's version and channel. Asks the feed nothing. |
| Check | One request. `up-to-date`, `available`, `declined`, `unavailable` (prerelease / unreadable) or `not-configured`. |
| Available | Names the version and the published notes, and says what is checked before the installer runs. Installing opens a `ConfirmDialog` first. |
| Install | Progress from the Rust side, then "Moddin will close and open again". |
| Declining | Records the version. The next check reports `declined` and the panel shows the current version, a note saying which version was turned down, and a button to show it again. No card, no prompt — and a remembered refusal is reversible, because a stray Escape must not lock the user out of a release until they clear their own storage. |

Failure, in all cases: nothing is installed, the app keeps running, and the
reason arrives through the one error path (`useFriendlyError` +
`ErrorCallout`). The messages are generated next to the classification in
`FailureKind::message()`, so the wording and the classification cannot drift.

### The restart is the installer's

On Windows `download_and_install` launches the NSIS installer with `/P /UPDATE
/R /ARGS` and then exits the process itself; the installer restarts Moddin.
That is why this feature has **no relaunch command and no
`tauri-plugin-process` dependency**: on the platform this app ships, a
relaunch path of our own would be a second path with no caller. If the
installer cannot be launched, the plugin returns the error *before* exiting
and the current version is untouched.

---

## Verifying a change

```bash
cd src-tauri && cargo fmt --check
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test --lib            # includes the decision rules
cd .. && npm run typecheck && npm run build
cd .. && npm run check:architecture
cd .. && npm run test                       # the panel's suite
```

`src-tauri/src/app_update.rs` holds the tests for the rules that matter:
offered by version comparison, prereleases refused, a declined version not
offered again, a signature failure classified as a refusal, every failure
message stating that nothing was installed.

### Dry run

Nothing here can be exercised end to end without a key. The first tag after
substituting one is the test: build it, install it on a spare machine, and
watch the panel offer it, then interrupt the install once and confirm the old
version still launches. A dry-run tag also has to be a **non-prerelease**
tag, or the updater will not see it at all — which is the channel rule
working, not a bug.
