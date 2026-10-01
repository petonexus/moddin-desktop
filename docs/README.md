# Moddin Desktop documentation

Start here. Every document in this repo, what it is for, and who should read it.

| Document | Read it when you want to… |
| --- | --- |
| [SCOPE.md](SCOPE.md) | Know what the project **is and is not** — Windows-only, local, not an anti-cheat bypass, not a Nexus client. One page, and it carries the honest status table. |
| [USER-GUIDE.md](USER-GUIDE.md) | Install Moddin and use it: first launch, applying a mod, the activity log, the community catalog, where your data lives. |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribute. Capability walkthrough, bug reports, translations, **and the conventions this repo follows** — branch names, commit types, what a commit body owes the reviewer. |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Build and run it: prerequisites, the bundled runtime you must generate before cargo will run, validation commands, adding a game. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Understand how the layers fit — Vue UI, declarative catalog, Tauri native layer, the action engine, transactions, action history. |
| [MODULES.md](MODULES.md) | Decide between a capability and a step kind, and write one. The canonical download-then-extract pattern, and why the older pattern failed. |
| [BACKEND-MODULES.md](BACKEND-MODULES.md) | Find out which of `obs.rs` / `obs_module.rs` (and the eight other pairs) is the file to edit. Read this before touching anything under `src-tauri/src/`. |
| [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md) | Look up a field: full YAML schema, all 11 step kinds and 6 check kinds, dependencies, compatibility, template rendering, the Tauri command surface. |
| [FRONTEND.md](FRONTEND.md) | Work in `src/`: component boundaries, state rules, styling, i18n, dialog lifecycle, the architecture guard and its App.vue budget. |
| [UPDATER.md](UPDATER.md) | Complete the app self-updater: which key goes where, what a release has to publish, and what a failed update leaves behind. Start here before changing anything under `src-tauri/capabilities/app-update.json` or `src/features/app-update/`. |
| [AGENT-RECIPES.md](AGENT-RECIPES.md) | Understand what the AI agent may and may not do, and why. Carries the repo's real threat model. |
| [AI-ASSISTANT.md](AI-ASSISTANT.md) | Connect a local AI (Codex, Claude Code, Cursor) to Moddin. *Written in Brazilian Portuguese.* |
| [CHEEKY-AI-RECIPE-RESEARCH.md](CHEEKY-AI-RECIPE-RESEARCH.md) | Read the research behind AI-assisted Cheeky recipes: evidence ladder, output contract, test protocol. *Portuguese.* |
| [cheeky-ai-recipe-template.yaml](cheeky-ai-recipe-template.yaml) | The recipe skeleton that research produces — a starting point, not a finished spec. |
| [AUDIT-2026-09-29.md](AUDIT-2026-09-29.md) | Check a claim. The dated audit that every `ROADMAP.md` item cites an id from. A **snapshot** — read it for the evidence, not for current status. |
| [release-notes/](release-notes/README.md) | Read what shipped in which beta, and publish the next one. |

## Outside this folder

| Document | What it is |
| --- | --- |
| [../ROADMAP.md](../ROADMAP.md) | What is shipping next. Every item cites the audit id it came from and carries a **Done when** line. |
| [../tools/README.md](../tools/README.md) | How to package a standalone Windows tool, and when a workflow belongs in `tools/` instead of in a capability. |
| [../moddin-agent/README.md](../moddin-agent/README.md) | The MCP server an AI assistant uses to author capabilities. |

## House rules for these documents

- **A document that describes a feature must say whether you can reach it.** A
  `status: planned` recipe, a Rust command with no caller, or a panel that does
  not exist yet gets labelled, not glossed. `SCOPE.md`'s status table and the
  "Reachable from the UI?" column in `MODULES.md` exist for this.
- **Every claim about code carries a path.** A claim you cannot point at
  `path:line` for is a claim to verify before you write it down.
- **English, and concise.** These are working documents. Two are deliberately in
  Brazilian Portuguese and are marked as such.
