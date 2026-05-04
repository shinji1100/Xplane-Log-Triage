# X-Plane Log Triage Tool

X-Plane Log Triage Tool is a local diagnostic helper for X-Plane 12.

The current preview has two modes:

- Log mode: analyze one `Log.txt`.
- Bundle mode: first collect a local diagnostic bundle, then analyze it with extra context.

No files are uploaded. The tool writes reports on your own machine.

## Easy Windows Preview

The easy zip contains:

```text
xplane-doctor.exe
1-collect.bat
2-analyze.bat
README.txt
```

The executable is still named `xplane-doctor.exe` in the preview build for compatibility, but the product name is X-Plane Log Triage Tool.

Use it like this:

1. Double-click `1-collect.bat`.
2. Double-click `2-analyze.bat`.
3. Open `doctor-report/report.html` if it does not open automatically.

The current test package assumes this X-Plane path:

```text
E:\SteamLibrary\steamapps\common\X-Plane 12
```

If your X-Plane is somewhere else, edit `1-collect.bat` and change the `XPLANE=` line.

## Command Line

Create a diagnostic bundle:

```powershell
xplane-doctor.exe collect "D:\X-Plane 12" --output ".\doctor-bundle"
```

Analyze a diagnostic bundle:

```powershell
xplane-doctor.exe analyze-bundle ".\doctor-bundle" --output ".\doctor-report"
```

Analyze only one `Log.txt`:

```powershell
xplane-doctor.exe analyze-log "D:\X-Plane 12\Log.txt" --output ".\doctor-report-log"
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

## Current Report Philosophy

The report is intentionally split into:

- Main Problems: likely actionable issues.
- Things To Check: lower-priority checks.
- Background / Technical Details: history and low-value technical noise.

Examples of background-only data:

- Global Airports `E/APT` or `W/APT` data quality messages.
- DSF road-network warnings from scenery packages.
- Texture packaging warnings such as non-power-of-two textures.
- Historical crash dumps that do not match the current `Log.txt`.
- `aftermath` files unless they match the current crash.

This keeps the report from making harmless log noise look like a serious crash cause.

## Developer Commands

```powershell
cargo check
cargo test
cargo build --release --bin xplane-doctor
```

The main CLI source is in:

```text
src/main.rs
src/lib.rs
src/rules.rs
src/report.rs
src/model.rs
```
