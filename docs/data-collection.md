# Data Collection Notes

X-Plane Log Triage Tool has two analysis paths:

- Log-only analysis for a single `Log.txt`.
- Diagnostic bundle analysis for local, richer evidence.

The diagnostic bundle is not meant to replace `Log.txt`. It adds context around the current log.

## Bundle Command

```powershell
xplane-doctor.exe collect "D:\X-Plane 12" --output ".\doctor-bundle"
```

The output folder contains:

```text
diagnostic-bundle.json
Log.redacted.txt
scenery_packs.redacted.ini
```

## Privacy Rules

By default, the collector does not include:

- Raw `.dmp` files.
- Full plugin folders.
- Full scenery packages.
- Preference file contents.
- Large binary files.

`Log.redacted.txt` replaces the selected X-Plane root path with `<xplane>` and redacts local user path segments.

`diagnostic-bundle.json` uses relative paths where possible.

## Crash Evidence Rules

Do not mix the current `Log.txt` with unrelated historical crash dumps.

The bundle records:

- Current log crash UUIDs.
- Historical Log Archive crash UUIDs.
- Crash dump file metadata.
- `matched_log_uuid`.
- `matched_log_source`.
- `matched_log_file`.
- `relation_to_current_log`.

Only a dump that matches the current log UUID should support a current-session crash conclusion.

Historical dump matches are background context.

## Log Archive

`Output/Log Archive` files are read with lossy UTF-8 text handling because X-Plane logs can contain odd or binary-looking telemetry lines.

`Log_ATC-*` files are tagged as `atc_log`. Their `clean_shutdown` field is `null` because ATC logs do not use the main `Log.txt` shutdown marker.

## Aftermath

`Output/crash_reports/aftermath` is scanned for file metadata only. Zip contents are not read or included.

An aftermath file is treated as background unless it can be tied to the current log.

## Report Priority

The report should avoid treating routine data-quality noise as a main problem.

Default background examples:

- Global Airports `E/APT` and `W/APT` messages.
- DSF road-network quality messages.
- Texture packaging warnings.
- Historical dump files.
- Historical aftermath files.
