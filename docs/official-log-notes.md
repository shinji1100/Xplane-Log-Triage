# Official X-Plane Log and Diagnostic Notes

Research date: 2026-05-04

This file records the official or first-party basis for X-Plane Log Triage Tool rules. Rules should prefer these sources before community heuristics.

## What official sources say about Log.txt

### Location and lifecycle

Official X-Plane material states that `Log.txt` is found in the X-Plane folder/root X-Plane folder.

The X-Plane Developer blog notes that the simulator must quit before `Log.txt` is completely written to disk.

Implication for the tool:

- If X-Plane is still running, a partial `Log.txt` should not be treated as complete.
- A missing `Log.txt` should only be reported when the file truly does not exist.
- A read failure should be reported separately from a missing file.

### Purpose

Official developer guidance says add-on authors should check `Log.txt`:

- when content does not look as expected
- when there is an error
- before posting work

It also says major package errors may be shown to the user once, while details are logged.

Implication for the tool:

- Log entries are not all errors.
- Informational load lines, capability lists, extension lists, and shutdown lines should not be reported as problems.
- The tool should focus on explicit error/warning markers or official failure phrases.

### Bug reports

The X-Plane bug report page says desktop simulator bug reports should include `Log.txt`.

For crash reports, it also asks for `.dmp` files in `Output/Crash Reports` if the automatic crash reporter appeared.

Implication for the tool:

- Forum/bug-report summaries should mention `Log.txt`.
- If a crash is detected, a future rule should check `Output/Crash Reports` for recent `.dmp` files.
- Add-on/plugin-caused issues should be reproduced with default aircraft/airport or without add-ons before being sent to Laminar.

### Safe Mode

Official X-Plane 12 bug-report guidance says Safe Mode can:

- lower graphics settings for the current session
- load only default aircraft
- use only default scenery, ignoring `Custom Scenery` and `scenery_packs.ini`
- ignore all plugins except aircraft-required plugins

Implication for the tool:

- Suggestions should prefer Safe Mode/default-aircraft/default-airport isolation before blaming X-Plane itself.
- Plugin/scenery rules should include a safe-mode or disabled-addons retest suggestion when appropriate.

## Official scenery priority rules

Official X-Plane scenery documentation says:

- `scenery_packs.ini` controls Custom Scenery load priority.
- Packs at the top have higher priority and override packs below.
- New scenery not already in the file is added to the top when X-Plane runs.
- Global Airports should be lower than custom airports but higher than base meshes.
- `SCENERY_PACK_DISABLED` disables a scenery pack.
- Users should not delete or modify default scenery packs just to fix conflicts.
- Users should not constantly delete `scenery_packs.ini`.

Implication for the tool:

- `SCENERY_PACK_DISABLED` is not an error.
- `*GLOBAL_AIRPORTS*` and similar special entries must not be treated as missing folders.
- The tool should not recommend deleting default scenery.
- The tool may flag obvious missing paths, but priority/order warnings need conservative wording.

## Official system requirements relevant to graphics rules

Official X-Plane 12 system requirements mention:

- Vulkan 1.3-capable GPU as a minimum video requirement.
- Windows 10/11 64-bit, macOS 12+, and Ubuntu Linux LTS.
- Supported GPU/driver baselines vary by vendor.

Implication for the tool:

- Vulkan support/capability lines are not errors by themselves.
- Official `VK_ERROR_*` codes are stronger evidence than loose words like `vulkan`.
- Hardware/driver suggestions should be phrased as compatibility checks, not definite root causes.

## Sources

- X-Plane Developer: Authors: Always Check Log.txt  
  https://developer.x-plane.com/2008/06/authors-always-check-log-txt/
- X-Plane Bug Report Form  
  https://www.x-plane.com/x-plane-bug-report-form/
- X-Plane: Prioritization of Scenery Packs  
  https://www.x-plane.com/kb/prioritization-scenery-packs/
- X-Plane: Benchmarking Using the Frame Rate Test  
  https://www.x-plane.com/kb/frame-rate-test/
- X-Plane 12 System Requirements  
  https://www.x-plane.com/kb/x-plane-12-system-requirements/
