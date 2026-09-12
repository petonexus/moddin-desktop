# Moddin

**Moddin** is a Windows-first desktop manager for PC game mods and tooling.

The goal is to replace one-off installers with a reusable, declarative system: Moddin detects installed games, shows supported recipes for each title, applies changes safely, and can roll them back later.

## Stack

- Tauri 2
- Vue 3
- TypeScript
- Rust only for native desktop operations
- YAML game/tool catalog

## MVP

The first milestone focuses on:

- detecting Steam libraries and installed games;
- matching installed games against Moddin's catalog;
- Elden Ring and Cyberpunk 2077 as the first supported titles;
- reusable modules, starting with OBS VR capture;
- transaction-based backup/rollback as the foundation for future installers.

## Development

Requirements on Windows:

- Node.js 22+
- Rust stable
- Tauri prerequisites / WebView2

Then:

```bash
npm install
npm run tauri dev
```

The current bootstrap work lives in `feat/bootstrap-mvp` until the first usable slice is ready to merge.

## Architecture principle

Game support should be data-driven whenever possible. Adding a game should mostly mean adding a catalog entry instead of adding hard-coded `if (game === ...)` branches throughout the app.

Native code is reserved for things that actually need native access: Steam discovery, filesystem/process operations, backups, Windows integration, and later OBS/tool configuration.

## License

GPL-3.0.
