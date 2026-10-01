# Development setup (Windows)

Moddin uses Tauri 2. The UI is Vue 3 + TypeScript, but Tauri itself requires Rust/Cargo and the Microsoft C++ toolchain to compile the native desktop shell.

## Fast path

Double-click `SETUP-WINDOWS.bat` from the repository root.

The helper:

1. checks `winget`;
2. installs Rustup when it is missing;
3. selects the `stable-msvc` Rust toolchain;
4. verifies that Cargo is available;
5. checks for Visual Studio / Microsoft C++ Build Tools;
6. opens the official Build Tools installer when the native C++ workload is missing.

If Rustup has just been installed, close the terminal and open a new one before continuing so `%PATH%` is refreshed.

## Manual prerequisites

### Rust

```powershell
winget install --id Rustlang.Rustup
rustup default stable-msvc
```

Restart the terminal and verify:

```powershell
rustc --version
cargo --version
```

### Microsoft C++ Build Tools

Install Visual Studio Build Tools 2022 and select **Desktop development with C++**, including the recommended MSVC toolset and Windows SDK.

### WebView2

Modern Windows 10/11 systems normally already include Microsoft Edge WebView2. Tauri uses it to render the desktop UI.

### Node.js

Needed for the frontend (`npm install`, `npm run tauri dev`) **and** for the
bundled runtime below, which stages the local-AI agent into the installer.

```powershell
winget install --id OpenJS.NodeJS.LTS
node --version
npm --version
```

### The bundled runtime is a prerequisite

`src-tauri/tauri.conf.json` declares two bundle resources:

```json
"resources": [
  "../moddin-runtime/node.exe",
  "../moddin-runtime/moddin-agent"
]
```

`tauri-build` validates those paths during cargo's **build script**, so they must
exist before anything native is compiled. On a fresh clone all of these fail with
an error about a missing resource — not about anything you changed:

```
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml
npm run tauri dev
npm run tauri build
```

Create the runtime once per clone:

```powershell
pwsh -NoProfile -File ./moddin-runtime/bundle-runtime.ps1
```

`moddin-runtime/` is generated and gitignored; only `bundle-runtime.ps1` and
`test-integration.ps1` in it are source. The script:

1. downloads portable Node `v20.19.5` and verifies it against the SHA-256
   `nodejs.org` publishes in `SHASUMS256.txt` next to the archive, aborting on a
   mismatch rather than extracting it;
2. stages a production-only copy of `moddin-agent/` with dev dependencies dropped;
3. installs that copy's production dependencies;
4. smoke-tests the staged server and writes `moddin-runtime/BUNDLE_OK`.

It needs Node on `PATH` — step 3 runs `npm install` — and network access to
`nodejs.org` and the npm registry. CI runs the identical step before
`cargo test` (`.github/workflows/ci.yml`); run it locally, or your Rust checks
fail for a reason the error message does not explain.

**Known gap.** Step 3 generates a fresh `package.json` with caret ranges and runs
`npm install --omit=dev`. There is no lockfile for the staged runtime, so that
part of the shipped bundle is **not** reproducible the way the rest of the repo
is: CI uses `npm ci` with a committed `package-lock.json` as the dependency
source of truth. The Node download is digest-verified; the dependency install is
not. Tracked as `O-06` in [ROADMAP.md](../ROADMAP.md).

## Run Moddin

```powershell
npm install
npm run tauri dev
```

If npm reports that `esbuild` has a pending install script under an `allow-scripts` policy, approve that package according to the command npm prints, then run `npm install` again.

`npm run tauri dev` also needs `moddin-runtime/` to exist — see
[The bundled runtime is a prerequisite](#the-bundled-runtime-is-a-prerequisite).

## Validate before committing

Frontend typechecking is part of the production build:

```powershell
npm run build
```

Native validation mirrors CI — **after** `bundle-runtime.ps1` has run at least
once, or the resource check in cargo's build script fails before your code is
even compiled:

```powershell
pwsh -NoProfile -File ./moddin-runtime/bundle-runtime.ps1   # once per clone
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

CI uses `npm ci` so `package-lock.json` is the dependency source of truth. Commit lockfile changes whenever dependencies change.

## Add a supported game

Game catalog files live under `src/catalog/games/*.yaml` and are discovered automatically at build time. Do **not** add a matching TypeScript import when creating a game recipe.

A new game should normally require only a YAML entry containing its stable catalog id, a Steam or Epic App ID, executable path and module declarations.

The catalog loader validates every YAML entry and fails fast when:

- the YAML does not match the catalog schema;
- two games use the same catalog `id`;
- two games use the same Steam App ID or Epic App ID.

This keeps game support declarative and prevents the frontend from becoming a list of hard-coded game imports.

## Frontend changes

Use `src/styles/index.css` as the stylesheet entrypoint. Shared visual decisions belong in `src/styles/tokens.css`; cross-component polish belongs in `src/styles/polish.css` while the legacy root stylesheet is being decomposed.

Do not append another redesign block to `src/style.css`. When extracting UI from `App.vue`, move the relevant component styles with the component and remove the obsolete selectors from the legacy file.

See [`FRONTEND.md`](FRONTEND.md) for component boundaries, state rules and the incremental refactor plan.

## Why Cargo is required if most code is TypeScript

The Vue/TypeScript side is the UI and application layer. Tauri compiles a small native Rust host that exposes filesystem, Steam/Epic detection, process, Windows, backup, and other privileged local operations to the frontend. You should not need to work in Rust for ordinary UI/catalog development, but the Rust toolchain is still required to build/run the desktop app locally.
