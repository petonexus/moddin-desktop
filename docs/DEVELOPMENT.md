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

## Run Moddin Desktop

```powershell
npm install
npm run tauri dev
```

If npm reports that `esbuild` has a pending install script under an `allow-scripts` policy, approve that package according to the command npm prints, then run `npm install` again.

## Validate before committing

Frontend typechecking is part of the production build:

```powershell
npm run build
```

Native validation mirrors CI:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

CI uses `npm ci` so `package-lock.json` is the dependency source of truth. Commit lockfile changes whenever dependencies change.

## Add a supported game

Game catalog files live under `src/catalog/games/*.yaml` and are discovered automatically at build time. Do **not** add a matching TypeScript import when creating a game recipe.

A new game should normally require only a YAML entry containing its stable catalog id, Steam App ID, executable path and module declarations.

The catalog loader validates every YAML entry and fails fast when:

- the YAML does not match the catalog schema;
- two games use the same catalog `id`;
- two games use the same Steam App ID.

This keeps game support declarative and prevents the frontend from becoming a list of hard-coded game imports.

## Frontend changes

Use `src/styles/index.css` as the stylesheet entrypoint. Shared visual decisions belong in `src/styles/tokens.css`; cross-component polish belongs in `src/styles/polish.css` while the legacy root stylesheet is being decomposed.

Do not append another redesign block to `src/style.css`. When extracting UI from `App.vue`, move the relevant component styles with the component and remove the obsolete selectors from the legacy file.

See [`FRONTEND.md`](FRONTEND.md) for component boundaries, state rules and the incremental refactor plan.

## Why Cargo is required if most code is TypeScript

The Vue/TypeScript side is the UI and application layer. Tauri compiles a small native Rust host that exposes filesystem, Steam detection, process, Windows, backup, and other privileged local operations to the frontend. You should not need to work in Rust for ordinary UI/catalog development, but the Rust toolchain is still required to build/run the desktop app locally.
