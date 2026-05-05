# Changelog

## 0.2.0

- Added broader Windows X-Plane 12 log triage rules based on real logs.
- Added GPU crash detection for `VK_ERROR_DEVICE_LOST`, Vulkan result codes, Aftermath `FILE` crash markers, and Chinese crash text.
- Added VRAM pressure severity handling, including high severity for extreme texture downscaling.
- Added third-party plugin error detection for plugin-owned `ERROR` log formats and timed request timeouts.
- Added `E/PLG` runtime plugin error detection.
- Added aircraft `.acf` open-failure detection for missing, corrupt, too-old, or unknown aircraft files.
- Added scenery coverage for `Missing object ... from package ...` lines.
- Tuned common startup/background noise such as `E/IDENT`, `E/NVAPI`, `E/SOUN`, and short early-loading logs.
- Added generic finding suppression so specific rules take priority over broad `E/...` and `W/...` subsystem scans.
- Updated README to describe current rule coverage and limits instead of implying open-ended diagnosis.

## 0.1.0 Preview

- Added local `Log.txt` analysis for common X-Plane 12 crash, plugin, scenery, Vulkan, and resource issues.
- Added diagnostic bundle collection and bundle analysis with redacted log/scenery evidence.
- Added HTML, JSON, and forum-summary report output.
- Added English/Chinese locale support for CLI and GUI surfaces.
- Added native GUI preview behind the `gui` feature.
- Added regression tests with synthetic and real-log corpus cases.
- Reduced noisy X-Plane subsystem findings such as airport database, plugin initialization, network beacon, and DSF background messages.
- Added crash-context hints for pre-crash log lines and scenery/airport loading stages.
