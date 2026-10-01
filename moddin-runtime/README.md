# moddin-runtime

The folder `src-tauri/tauri.conf.json` lists under `bundle.resources`, and
therefore the part of the installer that is not the application: a
**portable Node** and a **production-only copy of `moddin-agent`**, the MCP
server the user wires into Claude Code, Codex or Cursor from
*Settings → Local AI*.

Everything in here is generated. The only tracked file is
`bundle-runtime.ps1`; `node.exe`, `moddin-agent/`, `node.zip`, `tmp/` and
`BUNDLE_OK` are all in `.gitignore` because none of them is source.

```powershell
pwsh -NoProfile -File ./moddin-runtime/bundle-runtime.ps1
```

Run it before `npm run tauri build`, on a machine with Node and npm on
`PATH`. CI runs the same script (`ci.yml`, `release.yml`).

## What it does

1. **Node.** Downloads `node-v20.19.5-win-x64.zip`, checks it against the
   SHA-256 Node publishes beside it, extracts it, and keeps `node.exe`. A
   `node.exe` that is already there is left alone. The digest comes from
   the same host as the archive — that catches a corrupted or substituted
   download, not a compromised `nodejs.org` — and the version is pinned
   in the script rather than resolved.
2. **The agent.** Copies `moddin-agent/` (everything except its
   `node_modules`) and writes a production `package.json` over the copied
   dev one.
3. **The dependencies.** Installs them with `npm ci` and runs the
   agent's own smoke test against the staged copy.
4. **The sweep.** Deletes everything the run created, verifies what is
   left, and only then writes `BUNDLE_OK`.

`BUNDLE_OK` is the marker CI checks. A bundle without it is a bundle
nobody proved runs.

## Reproducible installs (ROADMAP O-06)

The archive is digest-verified, and until this pass the dependencies were
not: the script used to write a fresh `package.json` with `^` ranges and
run `npm install --omit=dev` with no lockfile, so two release builds a week
apart could ship different code under the same version number. The repo
states that rule for itself and was breaking it for its own artifact.

The blocker was that the generated manifest lives under a gitignored path,
so no lockfile could be committed next to it. It does not need to be. The
lockfile that matters already exists: `moddin-agent/package-lock.json` is
tracked, sits next to `moddin-agent/package.json`, and describes exactly
the dependency set that ships. So the staged folder **copies that
lockfile in** and installs with `npm ci`, which builds the tree from the
lock rather than from ranges the registry re-interprets on every run.

Two things follow, and both are asserted by the script:

- The staged `package.json` is generated with **exact** versions, read out
  of the committed lockfile. Not the carets it used to write. With a
  lockfile those carets would be inert, but a manifest that declares a
  range is a manifest a later edit can take at face value, and the install
  would then quietly stop matching the lock.
- Nothing is resolved against the registry. If the lockfile is missing, or
  does not resolve one of the four production dependencies, the script
  stops instead of falling back to a range.

**Reproducibility check** — two clean bundles must produce identical
`node_modules`:

```powershell
pwsh -NoProfile -File ./moddin-runtime/bundle-runtime.ps1
# hash every file under moddin-runtime/moddin-agent/node_modules
pwsh -NoProfile -File ./moddin-runtime/bundle-runtime.ps1
# hash again; the two hashes match
```

Verified with npm 11.17.0 / Node 24.19.0: 3,869 files, identical SHA-256
tree hash across two consecutive `npm ci` runs.

## What the script leaves behind: nothing

`tauri.conf.json` bundles the whole `moddin-agent` **directory**, so
anything left inside it ships. Three things used to be:

| Left behind | Size | How |
| --- | --- | --- |
| `node.zip` | 28.5 MB | extracted, then removed on a "best-effort" basis with a comment normalising the case where the removal did not happen |
| `tmp/` | ~79 MB | the extracted Node archive, same removal, same comment |
| `moddin-agent/node_modules.old` | 56 MB | the script renamed the previous tree out of the way and never deleted the rename — and the tree it renamed was the *dev* install copied out of the source folder, `@yao-pkg/pkg` and all |

All three are now removed unconditionally, and the script **throws** if
one of them survives rather than reporting success. A build that cannot
clean up after itself is a build whose installer grew for no reason, and
that should stop the release rather than pass quietly.

`ci.yml` re-checks the same three on the bundle it is about to build
against, because a cached bundle is a claim about work this run did not
do.
