# X-Plane Log Triage Tool

X-Plane Log Triage Tool is a local diagnostic helper for X-Plane 12 logs.

It does not try to guess every possible root cause. It applies a set of
explicit log rules, groups related evidence, and separates actionable findings
from common background noise.

The current preview has two modes:

- Log mode: analyze one `Log.txt`.
- Bundle mode: first collect a local diagnostic bundle, then analyze it with extra context.

No files are uploaded. The tool writes reports on your own machine.

## Windows Preview

The preview zip contains:

```text
xplane-log-triage.exe
xplane-log-triage-gui.exe
README.md
CHANGELOG.md
```

Use the GUI:

```text
xplane-log-triage-gui.exe
```

Or use it from PowerShell or Command Prompt:

```powershell
xplane-log-triage.exe analyze-log "D:\X-Plane 12\Log.txt" --output ".\triage-report"
```

Then open `triage-report\report.html`.

## Command Line

Create a diagnostic bundle:

```powershell
xplane-log-triage.exe collect "D:\X-Plane 12" --output ".\triage-bundle"
```

Analyze a diagnostic bundle:

```powershell
xplane-log-triage.exe analyze-bundle ".\triage-bundle" --output ".\triage-report"
```

Analyze only one `Log.txt`:

```powershell
xplane-log-triage.exe analyze-log "D:\X-Plane 12\Log.txt" --output ".\triage-report-log"
```

## Diagnostic Bundle Contents

The bundle writes:

```text
diagnostic-bundle.json
Log.redacted.txt
scenery_packs.redacted.ini
```

The bundle records metadata from standard X-Plane 12 folders:

- Root files such as `Log.txt`, `Log_ATC.txt`, `Cycle Dump.txt`, `debug.log`, and `Data.txt`.
- `Resources/plugins` folder names, platform folders, `.xpl` files, and plugin-root `.log` metadata.
- `Custom Scenery` folder names and `scenery_packs.ini` entries.
- Top-level aircraft folder metadata, `.acf` counts, and aircraft plugin folder samples.
- `Custom Data`, `Global Scenery`, and selected `Output` metadata.
- `Output/crash_reports/reports` `.dmp` metadata only.
- `Output/crash_reports/aftermath` file metadata only.

The bundle does not include raw `.dmp` files, full plugins, full scenery packages, preference file contents, or large binary files.

## Current Rule Coverage

The current rules can identify these log patterns:

- Explicit X-Plane crash markers, including `This application has crashed`,
  crash UUID markers, crash `FILE` markers, and "cannot continue running"
  style messages.
- Vulkan and graphics failures, including official `VK_ERROR_*` result codes,
  `VK_ERROR_DEVICE_LOST`, generic graphics API error lines, and texture/VRAM
  pressure such as severe texture downscaling or very low memory headroom.
- Plugin load failures, including `dlerror`, `failed to load`, Windows error
  code 126, and invalid Win32 application messages.
- X-Plane SDK misuse by plugins, including threading violations and plugins
  bypassing XPLM when calling SDK functions.
- FlyWithLua script error lines.
- Runtime plugin errors reported by X-Plane's `E/PLG` channel, such as
  `Plugin <name> encountered error`.
- Third-party plugin error lines that do not use X-Plane's normal `E/...`
  channels, including bracketed plugin `ERROR` messages and timed plugin
  request timeouts.
- Aircraft file load failures, including `Failed to open the following
  aircraft` and `Unknown aircraft ... .acf`.
- Scenery problems, including missing art assets, missing object files,
  missing scenery libraries, missing global scenery tiles, and duplicate
  scenery/airport warnings.
- Crash context hints, such as the last meaningful pre-crash line, recent
  scenery-loading context, and early silent crashes.
- Abnormally truncated long-running logs when there is no clean shutdown marker
  and no explicit crash marker.

These rules are evidence-based. A finding means the log contains a known
pattern, not that the tool has proven the entire causal chain.

## Current Limits

- The primary target is Windows X-Plane 12 logs. Some macOS-specific security
  lines are recognized only when the log itself looks like a macOS log, but
  this is not full macOS support.
- The tool does not read crash dump contents. It only records crash report and
  Aftermath file metadata.
- Generic subsystem channels such as `E/SYS`, `E/OBJ`, `E/GFX`, `W/SCN`,
  `E/APT`, and `W/APT` can be useful, but they are not always root causes.
  Specific rules should be trusted more than broad subsystem scans.
- Background startup noise, such as many Global Airports messages, NVIDIA
  permission probing, joystick calibration messages, or sound device lookup
  messages, may be reported at low priority but should not be treated as the
  main crash cause by itself.
- A clean report does not prove the installation is healthy; it only means the
  current rule set did not find known patterns in the supplied files.

## Current Report Philosophy

The report is intentionally split into:

- Main Problems: likely actionable issues.
- Things To Check: lower-priority checks.
- Background / Technical Details: history and low-value technical noise.

Examples of background-only data:

- Global Airports `E/APT` or `W/APT` data quality messages.
- NVIDIA `E/NVAPI` startup permission probing.
- Sound device lookup messages such as `E/SOUN`.
- Joystick calibration warnings such as `E/JOY`.
- DSF road-network warnings from scenery packages.
- Texture packaging warnings such as non-power-of-two textures.
- Historical crash dumps that do not match the current `Log.txt`.
- `aftermath` files unless they match the current crash.

This keeps the report from making harmless log noise look like a serious crash cause.

## Developer Commands

```powershell
cargo check
cargo test
cargo build --release --bin xplane-log-triage
cargo build --release --bin xplane-log-triage-gui --features gui
```

The main CLI source is in:

```text
src/main.rs
src/lib.rs
src/rules.rs
src/report.rs
src/model.rs
```
