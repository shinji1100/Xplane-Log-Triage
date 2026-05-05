# Mac Log Candidate Rules

This file is a parking lot for macOS-specific or cross-platform observations.
These notes are not wired into the current Windows-first analyzer.

## Case: Log (21).txt

Source file:

```text
D:\cs2\Log (21).txt
```

Basic environment:

- X-Plane 12.4.2-r2, Apple Silicon build.
- macOS 26.4.1.
- Apple M3 Max, Metal renderer.
- Aircraft: Boeing 777-300ER / B777 Extended.
- Not a classic macOS Gatekeeper/quarantine plugin-load failure.

Observed current-session signals:

- The log has no clean shutdown marker:
  `----- X-Plane has shut down -----`
- The log has no explicit X-Plane crash marker:
  `This application has crashed`
- The log ends mid-flight after about 2:54 of sim time, shortly after scenery/DSF load lines.
- This supports an `abrupt_termination` style finding, but the follow-up advice should be macOS-specific if this analyzer later supports Mac logs.

Useful evidence lines:

```text
FlyWithLua Error: Error in ... FmodIntegration.cpp, line 732: An invalid parameter was passed to this function.
Terrain radar plugin ERROR: can't find dataref "anim/64/switch"
Terrain radar plugin ERROR: can't find dataref "T7Avionics/irs/status"
0:06:01.160 E/PLG: B777 (SASL) called XPLMCreateInstance ... during a post-flightloop callback. This is discouraged
[BOEING 777/787 ERROR]: [CPP]: [MM]: "BoeingFmc" [SBParser] Failed to parse Takeoff Data...
[BOEING 777/787 INFO]: [CPP]: [MM]: "BoeingFmc" FMC Message: ... NOT IN DATABASE; Description: FILE NOT FOUND OR CANNOT BE OPENED
```

Likely report stance:

- Main conclusion should remain cautious: the log is truncated without a crash marker, so Log.txt alone does not prove the root cause.
- B777/SASL and FlyWithLua are useful check items, not confirmed crash causes.
- TerrainRadar missing B777 datarefs should be a check item only.
- Global Airports `E/APT`, `W/APT`, and traffic-flow `W/SCN` lines are background noise.
- Repeated weather lines such as `METAR from the future` are background noise unless the user reports a weather/live-weather problem.

False-positive notes for future Mac support:

- Do not treat these FlyWithLua status lines as macOS security blocking:

```text
FlyWithLua: Plugin Scripts Quarantine Dir: ...
FlyWithLua Info: Searching for Lua quarantined script files
FlyWithLua Info: The folder /Resources/plugins/FlyWithLua/Scripts (Quarantine)/ does not exist or it is empty.
```

- Real macOS security findings should require stronger wording such as code-signature failure, notarization failure, Gatekeeper blocking, damaged app/plugin wording, or a failed plugin load tied to quarantine.

Candidate Mac-specific abrupt-termination advice:

```text
If this was an unexpected crash on macOS, check Console.app and
~/Library/Logs/DiagnosticReports for recent X-Plane crash or hang reports.
Also retest with third-party plugins disabled and watch Activity Monitor for
memory pressure.
```

Possible future rule kinds:

- `mac_abrupt_termination`
- `macos_security_block`
- `aircraft_plugin_runtime_error`
- `plugin_missing_dataref`
- `global_airports_background_noise`

