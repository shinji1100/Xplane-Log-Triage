use crate::model::*;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const OUTPUT_FILE_LIMIT: usize = 12;
const AIRCRAFT_ACF_SAMPLE_LIMIT: usize = 8;
const AIRCRAFT_PLUGIN_SAMPLE_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub(crate) struct LogUuidMatch {
    uuid: String,
    source: String,
    file_name: Option<String>,
}

pub(crate) fn collect_diagnostic_bundle(
    xplane_path: &Path,
    output_dir: &Path,
) -> io::Result<DiagnosticBundle> {
    let log_path = xplane_path.join("Log.txt");
    let scenery_ini_path = xplane_path.join("Custom Scenery").join("scenery_packs.ini");
    let crash_reports_path = xplane_path.join("Output").join("crash_reports");
    let crash_report_dumps_path = crash_reports_path.join("reports");

    let mut redacted_files = Vec::new();
    let log = collect_log_snapshot(xplane_path, &log_path, output_dir, &mut redacted_files);
    let log_start_unix_seconds = log.inferred_start_unix_seconds;
    let log_end_unix_seconds = log.modified_at_unix_seconds;
    let current_log_crash_uuids = log.crash_uuids.clone();
    let scenery = collect_scenery_snapshot(
        xplane_path,
        &scenery_ini_path,
        output_dir,
        &mut redacted_files,
    );
    let output = collect_output_snapshot(xplane_path)?;
    let log_uuid_matches =
        collect_log_uuid_matches(&current_log_crash_uuids, &output.log_archive_files);

    Ok(DiagnosticBundle {
        manifest: BundleManifest {
            schema_version: 3,
            doctor_version: env!("CARGO_PKG_VERSION").to_string(),
            generated_at_unix_seconds: unix_now(),
            xplane_path: "<xplane>".to_string(),
            collection_scope: vec![
                "root files: Log.txt, Log_ATC.txt, Cycle Dump.txt, debug.log, Data.txt".to_string(),
                "Aircraft: folder-level metadata, .acf/plugin samples only".to_string(),
                "Custom Data: file metadata only".to_string(),
                "Custom Scenery: scenery_packs.ini plus folder names".to_string(),
                "Global Scenery: folder names only".to_string(),
                "Resources/plugins: folder structure, platform dirs, .xpl files, .log metadata"
                    .to_string(),
                "Output: Log Archive crash status, diagnostic reports, preferences metadata"
                    .to_string(),
                "Output/crash_reports/reports: .dmp metadata only".to_string(),
                "Output/crash_reports/aftermath: file metadata only".to_string(),
            ],
            collection_limits: CollectionLimits {
                original_dump_files_included: false,
                original_preference_files_included: false,
                max_output_files_per_category: OUTPUT_FILE_LIMIT,
                max_aircraft_file_samples_per_folder: AIRCRAFT_ACF_SAMPLE_LIMIT,
                max_aircraft_plugin_samples_per_folder: AIRCRAFT_PLUGIN_SAMPLE_LIMIT,
            },
        },
        root_files: collect_interesting_root_files(xplane_path)?,
        root: RootSnapshot {
            has_xplane_exe: xplane_path.join("X-Plane.exe").exists()
                || xplane_path.join("X-Plane").exists()
                || xplane_path.join("X-Plane.app").is_dir()
                || xplane_path
                    .join("Contents")
                    .join("MacOS")
                    .join("X-Plane")
                    .exists(),
            has_log_txt: log_path.exists(),
            has_aircraft_dir: xplane_path.join("Aircraft").is_dir(),
            has_custom_data_dir: xplane_path.join("Custom Data").is_dir(),
            has_resources_dir: xplane_path.join("Resources").is_dir(),
            has_plugins_dir: xplane_path.join("Resources").join("plugins").is_dir(),
            has_global_scenery_dir: xplane_path.join("Global Scenery").is_dir(),
            has_custom_scenery_dir: xplane_path.join("Custom Scenery").is_dir(),
            has_scenery_packs_ini: scenery_ini_path.exists(),
            has_output_dir: xplane_path.join("Output").is_dir(),
            has_crash_reports_dir: crash_reports_path.is_dir(),
            has_crash_report_dumps_dir: crash_report_dumps_path.is_dir(),
        },
        log,
        plugins: collect_plugin_snapshots(xplane_path)?,
        scenery,
        aircraft: collect_aircraft_snapshots(xplane_path)?,
        custom_data_files: collect_file_snapshots(
            xplane_path,
            &xplane_path.join("Custom Data"),
            80,
        )?,
        global_scenery_folders: collect_child_dir_names(&xplane_path.join("Global Scenery"))?,
        output,
        crash_reports: collect_crash_report_snapshots(
            xplane_path,
            &crash_report_dumps_path,
            &log_uuid_matches,
            log_start_unix_seconds,
            log_end_unix_seconds,
        )?,
        crash_aftermath_files: collect_file_snapshots(
            xplane_path,
            &crash_reports_path.join("aftermath"),
            OUTPUT_FILE_LIMIT,
        )?,
        privacy: PrivacySnapshot {
            original_dump_files_included: false,
            redacted_files,
            redaction_rules: vec![
                "X-Plane root path".to_string(),
                "USERPROFILE path".to_string(),
                "HOME path".to_string(),
                "Windows /Users/<name>/ path segment".to_string(),
            ],
        },
    })
}

fn collect_interesting_root_files(xplane_path: &Path) -> io::Result<Vec<FileSnapshot>> {
    let mut files = Vec::new();
    for name in [
        "Log.txt",
        "Log_ATC.txt",
        "Cycle Dump.txt",
        "debug.log",
        "Data.txt",
    ] {
        let path = xplane_path.join(name);
        if path.is_file() {
            files.push(file_snapshot(xplane_path, &path, name.to_string())?);
        }
    }
    Ok(files)
}

fn collect_log_snapshot(
    xplane_path: &Path,
    log_path: &Path,
    output_dir: &Path,
    redacted_files: &mut Vec<String>,
) -> LogSnapshot {
    let metadata = fs::metadata(log_path).ok();
    match read_text_lossy(log_path) {
        Ok(text) => {
            let redacted_file = "Log.redacted.txt";
            let redacted_text = redact_sensitive_text_for_root(&text, xplane_path);
            let _ = fs::write(output_dir.join(redacted_file), redacted_text);
            redacted_files.push(redacted_file.to_string());
            let crash_uuids = extract_crash_uuids(&text);

            LogSnapshot {
                present: true,
                read_error: None,
                redacted_file: Some(redacted_file.to_string()),
                size_bytes: metadata.as_ref().map(|meta| meta.len()),
                modified_at_unix_seconds: metadata
                    .as_ref()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(system_time_to_unix_seconds),
                line_count: text.lines().count(),
                max_elapsed_seconds: max_log_elapsed_seconds(&text),
                inferred_start_unix_seconds: infer_log_start_unix_seconds(
                    metadata
                        .as_ref()
                        .and_then(|meta| meta.modified().ok())
                        .and_then(system_time_to_unix_seconds),
                    max_log_elapsed_seconds(&text),
                ),
                clean_shutdown: log_has_clean_shutdown(&text),
                crashed: log_has_crash_marker(&text),
                crash_uuids,
                summary: summarize_log(&text),
            }
        }
        Err(err) => LogSnapshot {
            present: log_path.exists(),
            read_error: Some(err.to_string()),
            redacted_file: None,
            size_bytes: metadata.as_ref().map(|meta| meta.len()),
            modified_at_unix_seconds: metadata
                .as_ref()
                .and_then(|meta| meta.modified().ok())
                .and_then(system_time_to_unix_seconds),
            line_count: 0,
            max_elapsed_seconds: None,
            inferred_start_unix_seconds: None,
            clean_shutdown: false,
            crashed: false,
            crash_uuids: Vec::new(),
            summary: SystemSummary::default(),
        },
    }
}

fn collect_plugin_snapshots(xplane_path: &Path) -> io::Result<Vec<PluginSnapshot>> {
    let plugins_path = xplane_path.join("Resources").join("plugins");
    if !plugins_path.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    for entry in fs::read_dir(&plugins_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let plugin_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let mut platform_dirs = Vec::new();
        let mut xpl_files = Vec::new();
        let mut log_files = Vec::new();

        for child in fs::read_dir(&plugin_path)? {
            let child = child?;
            let child_path = child.path();
            let child_name = child.file_name().to_string_lossy().to_string();
            if child.file_type()?.is_dir() {
                if is_platform_plugin_dir(&child_name) {
                    platform_dirs.push(child_name.clone());
                }

                for nested in fs::read_dir(&child_path)? {
                    let nested = nested?;
                    if nested.file_type()?.is_file() && has_extension(&nested.path(), "xpl") {
                        xpl_files.push(format!(
                            "{}/{}",
                            child_name,
                            nested.file_name().to_string_lossy()
                        ));
                    }
                }
            } else if has_extension(&child_path, "xpl") {
                xpl_files.push(child_name);
            } else if has_extension(&child_path, "log") {
                log_files.push(file_snapshot(xplane_path, &child_path, child_name)?);
            }
        }

        platform_dirs.sort();
        xpl_files.sort();
        log_files.sort_by(|left, right| left.name.cmp(&right.name));
        plugins.push(PluginSnapshot {
            name,
            path: relative_path_string(xplane_path, &plugin_path),
            modified_at_unix_seconds: entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(system_time_to_unix_seconds),
            platform_dirs,
            xpl_files,
            log_files,
        });
    }

    plugins.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(plugins)
}

fn collect_aircraft_snapshots(xplane_path: &Path) -> io::Result<Vec<AircraftSnapshot>> {
    let aircraft_path = xplane_path.join("Aircraft");
    if !aircraft_path.exists() {
        return Ok(Vec::new());
    }

    let mut aircraft = Vec::new();
    for entry in fs::read_dir(&aircraft_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let path = entry.path();
        let mut aircraft_files = Vec::new();
        let mut plugin_dirs = Vec::new();
        collect_aircraft_dir_summary(xplane_path, &path, &mut aircraft_files, &mut plugin_dirs, 0)?;
        aircraft_files.sort();
        plugin_dirs.sort();
        let acf_file_count = aircraft_files.len();
        let plugin_dir_count = plugin_dirs.len();
        let samples_truncated = acf_file_count > AIRCRAFT_ACF_SAMPLE_LIMIT
            || plugin_dir_count > AIRCRAFT_PLUGIN_SAMPLE_LIMIT;
        aircraft_files.truncate(AIRCRAFT_ACF_SAMPLE_LIMIT);
        plugin_dirs.truncate(AIRCRAFT_PLUGIN_SAMPLE_LIMIT);

        aircraft.push(AircraftSnapshot {
            name: entry.file_name().to_string_lossy().to_string(),
            path: relative_path_string(xplane_path, &path),
            modified_at_unix_seconds: entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(system_time_to_unix_seconds),
            acf_file_count,
            acf_file_samples: aircraft_files,
            plugin_dir_count,
            plugin_dir_samples: plugin_dirs,
            samples_truncated,
        });
    }

    aircraft.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(aircraft)
}

fn collect_aircraft_dir_summary(
    xplane_path: &Path,
    path: &Path,
    aircraft_files: &mut Vec<String>,
    plugin_dirs: &mut Vec<String>,
    depth: usize,
) -> io::Result<()> {
    if depth > 2 {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child_path = entry.path();
        let child_name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type()?.is_dir() {
            if child_name.eq_ignore_ascii_case("plugins") {
                plugin_dirs.push(relative_path_string(xplane_path, &child_path));
            }
            collect_aircraft_dir_summary(
                xplane_path,
                &child_path,
                aircraft_files,
                plugin_dirs,
                depth + 1,
            )?;
        } else if has_extension(&child_path, "acf") {
            aircraft_files.push(relative_path_string(xplane_path, &child_path));
        }
    }

    Ok(())
}

fn collect_scenery_snapshot(
    xplane_path: &Path,
    ini_path: &Path,
    output_dir: &Path,
    redacted_files: &mut Vec<String>,
) -> ScenerySnapshot {
    let custom_scenery_path = xplane_path.join("Custom Scenery");
    let custom_scenery_folders = collect_child_dir_names(&custom_scenery_path).unwrap_or_default();

    let Ok(text) = fs::read_to_string(ini_path) else {
        return ScenerySnapshot {
            ini_present: ini_path.exists(),
            read_error: if ini_path.exists() {
                Some("failed to read scenery_packs.ini".to_string())
            } else {
                None
            },
            redacted_ini_file: None,
            entries: Vec::new(),
            custom_scenery_folders,
            missing_active_paths: Vec::new(),
            duplicate_active_paths: Vec::new(),
            disabled_count: 0,
            special_entries: Vec::new(),
        };
    };

    let redacted_file = "scenery_packs.redacted.ini";
    let _ = fs::write(
        output_dir.join(redacted_file),
        redact_sensitive_text_for_root(&text, xplane_path),
    );
    redacted_files.push(redacted_file.to_string());

    let mut entries = Vec::new();
    let mut active_paths = BTreeMap::<String, usize>::new();
    let mut missing_active_paths = Vec::new();
    let mut disabled_count = 0;
    let mut special_entries = Vec::new();

    let mut order = 0;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if !line.starts_with("SCENERY_PACK") {
            continue;
        }

        let disabled = line.starts_with("SCENERY_PACK_DISABLED");
        let path = parse_scenery_path(line)
            .trim_end_matches('/')
            .trim_end_matches('\\')
            .to_string();
        if path.is_empty() {
            continue;
        }

        let special_entry = path.starts_with('*') && path.ends_with('*');
        let exists = special_entry || xplane_path.join(&path).exists();

        if disabled {
            disabled_count += 1;
        } else if special_entry {
            special_entries.push(path.clone());
        } else {
            *active_paths.entry(path.to_ascii_lowercase()).or_insert(0) += 1;
            if !exists {
                missing_active_paths.push(path.clone());
            }
        }

        entries.push(SceneryPackSnapshot {
            order,
            path: redact_sensitive_text(&path),
            disabled,
            special_entry,
            exists,
        });
        order += 1;
    }

    let duplicate_active_paths = active_paths
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(path, count)| DuplicatePathSnapshot { path, count })
        .collect();

    ScenerySnapshot {
        ini_present: true,
        read_error: None,
        redacted_ini_file: Some(redacted_file.to_string()),
        entries,
        custom_scenery_folders,
        missing_active_paths,
        duplicate_active_paths,
        disabled_count,
        special_entries,
    }
}

fn collect_output_snapshot(xplane_path: &Path) -> io::Result<OutputSnapshot> {
    let output_path = xplane_path.join("Output");
    if !output_path.exists() {
        return Ok(OutputSnapshot {
            directories: Vec::new(),
            log_archive_files: Vec::new(),
            diagnostic_report_files: Vec::new(),
            preference_files: Vec::new(),
        });
    }

    let directories = collect_child_dir_names(&output_path)?;
    Ok(OutputSnapshot {
        directories,
        log_archive_files: collect_log_archive_snapshots(
            xplane_path,
            &output_path.join("Log Archive"),
            OUTPUT_FILE_LIMIT,
        )?,
        diagnostic_report_files: collect_file_snapshots(
            xplane_path,
            &output_path.join("diagnostic reports"),
            OUTPUT_FILE_LIMIT,
        )?,
        preference_files: collect_file_snapshots(
            xplane_path,
            &output_path.join("preferences"),
            OUTPUT_FILE_LIMIT,
        )?,
    })
}

pub(crate) fn collect_crash_report_snapshots(
    xplane_path: &Path,
    crash_report_dumps_path: &Path,
    log_uuid_matches: &[LogUuidMatch],
    log_start_unix_seconds: Option<u64>,
    log_end_unix_seconds: Option<u64>,
) -> io::Result<Vec<CrashReportSnapshot>> {
    if !crash_report_dumps_path.exists() {
        return Ok(Vec::new());
    }

    let mut reports = Vec::new();
    for entry in fs::read_dir(crash_report_dumps_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || !has_extension(&entry.path(), "dmp") {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified_at_unix_seconds = metadata
            .modified()
            .ok()
            .and_then(system_time_to_unix_seconds);
        let dump_uuid = entry
            .path()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_ascii_lowercase());
        let matched_log = dump_uuid
            .as_deref()
            .and_then(|uuid| find_log_uuid_match(uuid, log_uuid_matches));
        let relation_to_current_log = if matched_log
            .as_ref()
            .is_some_and(|matched| matched.source == "current_log")
        {
            "uuid_matched".to_string()
        } else {
            crash_report_relation(
                modified_at_unix_seconds,
                log_start_unix_seconds,
                log_end_unix_seconds,
            )
        };
        reports.push(CrashReportSnapshot {
            file_name: entry.file_name().to_string_lossy().to_string(),
            path: relative_path_string(xplane_path, &entry.path()),
            size_bytes: metadata.len(),
            modified_at_unix_seconds,
            relation_to_current_log,
            matched_log_uuid: matched_log.as_ref().map(|matched| matched.uuid.clone()),
            matched_log_source: matched_log.as_ref().map(|matched| matched.source.clone()),
            matched_log_file: matched_log
                .as_ref()
                .and_then(|matched| matched.file_name.clone()),
            seconds_from_log_start: seconds_between(
                modified_at_unix_seconds,
                log_start_unix_seconds,
            ),
            seconds_from_log_end: seconds_between(modified_at_unix_seconds, log_end_unix_seconds),
        });
    }

    reports.sort_by(|left, right| {
        right
            .modified_at_unix_seconds
            .cmp(&left.modified_at_unix_seconds)
    });
    Ok(reports)
}

pub(crate) fn collect_log_uuid_matches(
    current_log_crash_uuids: &[String],
    log_archive_files: &[LogArchiveSnapshot],
) -> Vec<LogUuidMatch> {
    let mut matches = Vec::new();
    for uuid in current_log_crash_uuids {
        push_log_uuid_match(
            &mut matches,
            LogUuidMatch {
                uuid: uuid.clone(),
                source: "current_log".to_string(),
                file_name: Some("Log.txt".to_string()),
            },
        );
    }

    for archive in log_archive_files {
        if archive.kind != "main_log" {
            continue;
        }
        for uuid in &archive.crash_uuids {
            push_log_uuid_match(
                &mut matches,
                LogUuidMatch {
                    uuid: uuid.clone(),
                    source: "log_archive".to_string(),
                    file_name: Some(archive.name.clone()),
                },
            );
        }
    }

    matches
}

fn push_log_uuid_match(matches: &mut Vec<LogUuidMatch>, candidate: LogUuidMatch) {
    if matches
        .iter()
        .any(|existing| existing.uuid.eq_ignore_ascii_case(&candidate.uuid))
    {
        return;
    }

    matches.push(candidate);
}

fn find_log_uuid_match(uuid: &str, matches: &[LogUuidMatch]) -> Option<LogUuidMatch> {
    matches
        .iter()
        .find(|candidate| candidate.uuid.eq_ignore_ascii_case(uuid))
        .cloned()
}

fn collect_child_dir_names(path: &Path) -> io::Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    Ok(names)
}

fn collect_log_archive_snapshots(
    _root: &Path,
    path: &Path,
    limit: usize,
) -> io::Result<Vec<LogArchiveSnapshot>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let kind = log_archive_kind(&name).to_string();
        let text = read_text_lossy(&entry.path()).unwrap_or_default();
        let clean_shutdown = if kind == "main_log" {
            Some(log_has_clean_shutdown(&text))
        } else {
            None
        };
        files.push(LogArchiveSnapshot {
            name,
            kind,
            size_bytes: metadata.len(),
            modified_at_unix_seconds: metadata
                .modified()
                .ok()
                .and_then(system_time_to_unix_seconds),
            clean_shutdown,
            crash_uuids: extract_crash_uuids(&text),
        });
    }

    files.sort_by(|left, right| {
        right
            .modified_at_unix_seconds
            .cmp(&left.modified_at_unix_seconds)
    });
    files.truncate(limit);
    Ok(files)
}

pub(crate) fn log_archive_kind(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower == "log.txt" || (lower.starts_with("log-") && lower.ends_with(".txt")) {
        "main_log"
    } else if lower == "log_atc.txt" || (lower.starts_with("log_atc-") && lower.ends_with(".txt")) {
        "atc_log"
    } else {
        "unknown"
    }
}

fn collect_file_snapshots(root: &Path, path: &Path, limit: usize) -> io::Result<Vec<FileSnapshot>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(file_snapshot(
                root,
                &entry.path(),
                entry.file_name().to_string_lossy().to_string(),
            )?);
        }
    }
    files.sort_by(|left, right| {
        right
            .modified_at_unix_seconds
            .cmp(&left.modified_at_unix_seconds)
    });
    files.truncate(limit);
    Ok(files)
}

fn file_snapshot(root: &Path, path: &Path, name: String) -> io::Result<FileSnapshot> {
    let metadata = fs::metadata(path)?;
    Ok(FileSnapshot {
        name,
        path: relative_path_string(root, path),
        size_bytes: metadata.len(),
        modified_at_unix_seconds: metadata
            .modified()
            .ok()
            .and_then(system_time_to_unix_seconds),
    })
}

fn max_log_elapsed_seconds(log_text: &str) -> Option<f64> {
    log_text
        .lines()
        .filter_map(parse_elapsed_seconds_prefix)
        .max_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
}

fn parse_elapsed_seconds_prefix(line: &str) -> Option<f64> {
    let token = line.split_whitespace().next()?;
    let mut parts = token.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn infer_log_start_unix_seconds(
    log_modified_unix_seconds: Option<u64>,
    max_elapsed_seconds: Option<f64>,
) -> Option<u64> {
    let modified = log_modified_unix_seconds?;
    let elapsed = max_elapsed_seconds?;
    Some(modified.saturating_sub(elapsed.ceil() as u64))
}

pub(crate) fn extract_crash_uuids(log_text: &str) -> Vec<String> {
    const MARKER: &str = "--=={UUID:";
    const TERMINATOR: &str = "}==--";

    let mut uuids: Vec<String> = Vec::new();
    let mut search_start = 0;
    while let Some(marker_offset) = log_text[search_start..].find(MARKER) {
        let uuid_start = search_start + marker_offset + MARKER.len();
        let Some(terminator_offset) = log_text[uuid_start..].find(TERMINATOR) else {
            break;
        };
        let uuid = log_text[uuid_start..uuid_start + terminator_offset].trim();
        if is_uuid(uuid) && !uuids.iter().any(|seen| seen.eq_ignore_ascii_case(uuid)) {
            uuids.push(uuid.to_ascii_lowercase());
        }
        search_start = uuid_start + terminator_offset + TERMINATOR.len();
    }

    uuids
}

fn is_uuid(input: &str) -> bool {
    if input.len() != 36 {
        return false;
    }

    input.chars().enumerate().all(|(idx, ch)| match idx {
        8 | 13 | 18 | 23 => ch == '-',
        _ => ch.is_ascii_hexdigit(),
    })
}

fn log_has_clean_shutdown(log_text: &str) -> bool {
    log_text
        .lines()
        .any(|line| line.contains("----- X-Plane has shut down -----"))
}

pub(crate) fn log_has_crash_marker(log_text: &str) -> bool {
    if !extract_crash_uuids(log_text).is_empty() {
        return true;
    }

    log_text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("this application has crashed")
            || lower.contains("--=={uuid:")
            || lower.contains("forwarding exception")
            || lower.contains("x-plane cannot continue running")
            || lower.contains("thread fatal assert")
    })
}

fn crash_report_relation(
    dump_modified_unix_seconds: Option<u64>,
    log_start_unix_seconds: Option<u64>,
    log_end_unix_seconds: Option<u64>,
) -> String {
    const TOLERANCE_SECONDS: u64 = 120;
    let Some(dump_time) = dump_modified_unix_seconds else {
        return "unknown".to_string();
    };
    let (Some(start), Some(end)) = (log_start_unix_seconds, log_end_unix_seconds) else {
        return "unknown".to_string();
    };

    if dump_time + TOLERANCE_SECONDS < start {
        "before_current_log".to_string()
    } else if dump_time > end + TOLERANCE_SECONDS {
        "after_current_log".to_string()
    } else {
        "current_log_window".to_string()
    }
}

fn seconds_between(left: Option<u64>, right: Option<u64>) -> Option<i64> {
    Some(left? as i64 - right? as i64)
}

fn is_platform_plugin_dir(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "64" | "32" | "win_x64" | "win_x86" | "mac_x64" | "mac_x86" | "lin_x64" | "lin_x86"
    )
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn system_time_to_unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn relative_path_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| redact_sensitive_text(&path.display().to_string()))
}
fn redact_sensitive_text_for_root(input: &str, root: &Path) -> String {
    let root_backslash = root.display().to_string();
    let root_slash = root_backslash.replace('\\', "/");
    let root_mixed = root_slash.replacen(":/", ":\\", 1);
    let mut out = input.replace(&root_backslash, "<xplane>");
    out = out.replace(&root_slash, "<xplane>");
    out = out.replace(&root_mixed, "<xplane>");
    redact_sensitive_text(&out)
}

fn redact_sensitive_text(input: &str) -> String {
    let mut out = input.to_string();
    for var in ["USERPROFILE", "HOME"] {
        if let Some(value) = std::env::var_os(var) {
            let value = value.to_string_lossy();
            if !value.is_empty() {
                out = out.replace(value.as_ref(), "~");
                out = out.replace(&value.replace('\\', "/"), "~");
            }
        }
    }

    redact_windows_user_paths(&out)
}

fn redact_windows_user_paths(input: &str) -> String {
    let normalized = input.replace('\\', "/");
    let Some(marker_idx) = normalized.to_ascii_lowercase().find("/users/") else {
        return input.to_string();
    };
    let name_start = marker_idx + "/users/".len();
    let Some(name_end_rel) = normalized[name_start..].find('/') else {
        return input.to_string();
    };
    let name_end = name_start + name_end_rel;
    let username = &normalized[name_start..name_end];
    input
        .replace(&format!("\\Users\\{username}\\"), "\\Users\\<user>\\")
        .replace(&format!("/Users/{username}/"), "/Users/<user>/")
}
pub(crate) fn read_text_lossy(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn summarize_log(log_text: &str) -> SystemSummary {
    let mut summary = SystemSummary::default();

    for line in log_text.lines().take(250) {
        if summary.xplane_version.is_none() && line.starts_with("Log.txt for X-Plane") {
            summary.xplane_version = Some(line.trim().to_string());
        }
        if summary.os.is_none()
            && (line.starts_with("OS:") || line.contains("Windows") || line.contains("macOS"))
        {
            summary.os = Some(line.trim().to_string());
        }
        if summary.cpu.is_none() && (line.starts_with("CPU type:") || line.starts_with("CPU:")) {
            summary.cpu = Some(line.trim().to_string());
        }
        if summary.gpu.is_none()
            && (line.contains("GPU")
                || line.contains("OpenGL Renderer")
                || line.contains("Vulkan Device"))
        {
            summary.gpu = Some(line.trim().to_string());
        }
        if summary.aircraft.is_none()
            && (line.to_ascii_lowercase().contains("loading aircraft")
                || line.to_ascii_lowercase().contains(".acf"))
        {
            summary.aircraft = Some(line.trim().to_string());
        }
    }

    summary
}
pub(crate) fn parse_scenery_path(line: &str) -> &str {
    line.strip_prefix("SCENERY_PACK_DISABLED")
        .or_else(|| line.strip_prefix("SCENERY_PACK"))
        .unwrap_or("")
        .trim()
}
