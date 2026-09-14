# Game support matrix

| Game | Steam App ID | Executable | Modules | Notes |
| --- | --- | --- | --- | --- |
| DOOM (2016) | 379720 | DOOMx64.exe | Desktop Shortcut: available; KHARVOX VR: planned | Steam manifest and executable path verified locally. The shortcut action uses the game executable as both launch target and icon source and records an undo transaction. KHARVOX 0.8 Beta remains an externally managed launcher until Moddin gains a managed external-launcher capability. |

## Reusable requirements

- A desktop shortcut action must only accept the catalogued executable below the selected game directory, preview its target/icon, and create a transaction so replacing or creating the .lnk can be undone.
