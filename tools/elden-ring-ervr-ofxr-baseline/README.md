# Elden Ring — ERVR + VDXR + OFXR baseline

Reversible baseline package for the Elden Ring VR pipeline
**ERVR → OpenXR / VDXR → OFXR Bridge → Meta Quest 3** (or any
OpenXR-compatible headset).

This tool **inspects** the existing ERVR / ReShade / OpenXR / OFXR chain,
quarantines identified experimental extras without deleting them, and
restores the previous state through `02-RESTAURAR-ESTADO-ANTERIOR.cmd`.

It is intentionally **not** an installer for ERVR, OFXR, or DLSS. Those
live in the Moddin Desktop app (`ofxr`, `uevr`, `optiscaler`, `cheeky`
modules).

## Usage

From the repository root:

```bat
tools\elden-ring-ervr-ofxr-baseline\00-VERIFICAR-BASELINE.cmd
tools\elden-ring-ervr-ofxr-baseline\01-APLICAR-BASELINE.cmd
tools\elden-ring-ervr-ofxr-baseline\02-RESTAURAR-ESTADO-ANTERIOR.cmd
```

Each `.cmd` accepts an optional `-GamePath "<absolute path>"` argument to
override the auto-detected Steam install. The game **must be closed**
before applying or restoring.

| Script | Mode | What it does |
| --- | --- | --- |
| `00-VERIFICAR-BASELINE.cmd` | `Verify` | Read-only. Shows ERVR / ReShade / OpenXR / OFXR / Cheeky state and any extras still active. Writes a report next to the snapshot directory. |
| `01-APLICAR-BASELINE.cmd` | `Apply` | Snapshots `ERVR.ini`, moves experimental extras into a reversible quarantine, applies the conservative `ERVR.ini` baseline, disables the global Cheeky OpenXR layer. **Does not** touch OFXR or anti-cheat. |
| `02-RESTAURAR-ESTADO-ANTERIOR.cmd` | `Restore` | Returns quarantined files only if their original path is free, restores the previous `ERVR.ini`, restores the Cheeky OpenXR layer values. |

## Baseline values applied to `ERVR.ini`

```ini
[VR]          StereoMode=full | GameRes=auto | GameResScale=1.0 | RenderScale=1.0
[Patches]     FpsTarget=60
[FirstPerson] CameraBob=0
[HUD]         Mode=quad | Lock=body
[General]     LogLevel=debug
```

These match the values documented in [`MODDIN-AGENT-CONTEXT.md`](MODDIN-AGENT-CONTEXT.md)
and verified to work on the reference PC (RTX 4080 Super + Quest 3 via
Virtual Desktop / VDXR, ERVR 0.4.0, Elden Ring 2.7.1.0 / patch 1.17.1).

## State layout

Runtime artifacts are created **outside the Git tree**:

```text
ELDEN RING\_MODDIN_BACKUPS\ERVR-OFXR-Baseline\
└── snapshot-<YYYYMMDD-HHmmss>\
    ├── ERVR.ini.bak
    ├── quarantine\        # experimental extras moved here, never deleted
    └── BASELINE-REPORT.txt
```

## What the tool will not do

- Install, update, or reconfigure OFXR Bridge.
- Install DLSS runtimes, Streamline, OptiScaler, ERSS-FG, or DLSS Frame
  Generation add-ons.
- Touch `dxgi.dll`, `dinput8.dll`, or the `ERVR/` directory beyond
  editing `ERVR.ini`.
- Disable or bypass Easy Anti-Cheat. ERVR requires the user's existing
  offline / EAC-disabled launch method.
- Stack two frame generators at once (OFXR + ERSS-FG is forbidden by
  design).

## See also

- [`MODDIN-AGENT-CONTEXT.md`](MODDIN-AGENT-CONTEXT.md) — full baseline
  rationale, experiments history, and crash analysis.
- [`../README.md`](../README.md) — contract that all tools must follow.
- [`../../docs/SCOPE.md`](../../docs/SCOPE.md) — how tools relate to the
  Moddin Desktop app and its Rust modules.