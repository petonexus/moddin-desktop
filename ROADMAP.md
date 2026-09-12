# Roadmap

## v0.1 — Library bootstrap

- [x] Tauri + Vue + TypeScript skeleton
- [x] Steam library discovery
- [x] Windows Registry fallback for non-standard Steam installs
- [x] installed-game scan
- [x] YAML catalog validation
- [x] Elden Ring catalog entry
- [x] Cyberpunk 2077 catalog entry
- [x] installed-games UI
- [x] transaction model and local transaction store
- [x] first reusable module: OBS VR capture

## v0.2 — Safe actions

- [ ] generic action engine primitives
- [x] backup before mutation
- [x] rollback / undo
- [x] dry-run / preview screen
- [x] inspect executable directory for common proxy DLLs
- [x] VR-ready launch profiles with OpenXR preflight and recommended graphics settings
- [x] per-game OpenXR runtime override without changing the Windows global runtime
- [x] module verification checklist and post-action validation
- [x] reapply/reinstall and transaction-backed removal flows
- [ ] structured per-action logs
- [ ] conflict resolution for common proxy DLL names

## v0.3 — Graphics & VR modules

- [x] OptiScaler
- [x] OpenXR helpers
- [ ] ReShade
- [ ] per-game VR / flat profiles
- [x] module state detection and version reporting

## v0.4 — Mod frameworks

- [ ] UE4SS
- [ ] BepInEx
- [ ] REFramework
- [ ] mod loaders / framework dependency graph
- [ ] compatibility metadata by game build

## Later

- [x] GitHub Releases updater
- [ ] signed/versioned remote catalog
- [ ] Nexus integration where permitted
- [ ] Epic / GOG detection
- [ ] import/export profiles
