use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod bundle_analysis;
mod collect;
mod detect;
pub mod i18n;
mod model;
mod report;
mod rules;

pub use i18n::{get_locale, is_zh, set_locale, Locale};

use bundle_analysis::{
    analyze_bundle_input_quality_ascii, analyze_bundle_session_status_ascii,
    read_optional_bundle_text,
};
use collect::{collect_diagnostic_bundle, parse_scenery_path, read_text_lossy, summarize_log};

#[cfg(test)]
use collect::{
    collect_crash_report_snapshots, collect_log_uuid_matches, extract_crash_uuids,
    log_archive_kind, log_has_crash_marker, unix_now,
};
pub use detect::{choose_detected_install, detect_xplane_installs};
pub use model::*;
use report::{render_forum_summary, render_html, render_json};
pub use rules::analyze_log;

pub fn collect_diagnostic_bundle_to(
    xplane_path: &Path,
    output_dir: &Path,
) -> io::Result<DiagnosticBundle> {
    fs::create_dir_all(output_dir)?;
    let bundle = collect_diagnostic_bundle(xplane_path, output_dir)?;
    fs::write(
        output_dir.join("diagnostic-bundle.json"),
        serde_json::to_string_pretty(&bundle).expect("DiagnosticBundle should always serialize"),
    )?;
    Ok(bundle)
}

pub fn default_report_dir() -> std::path::PathBuf {
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        return std::path::PathBuf::from(user_profile)
            .join("Documents")
            .join("X-Plane Log Triage Reports");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home)
            .join("Documents")
            .join("X-Plane Log Triage Reports");
    }

    std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
}

pub fn scan_and_write_reports(xplane_path: &Path) -> io::Result<ScanReport> {
    scan_and_write_reports_to(xplane_path, &default_report_dir())
}

pub fn scan_and_write_reports_to(xplane_path: &Path, output_dir: &Path) -> io::Result<ScanReport> {
    let report = scan_xplane(xplane_path)?;
    fs::create_dir_all(output_dir)?;
    fs::write(output_dir.join("report.html"), render_html(&report))?;
    fs::write(output_dir.join("report.json"), render_json(&report))?;
    fs::write(
        output_dir.join("forum-summary.txt"),
        render_forum_summary(&report),
    )?;
    Ok(report)
}

pub fn analyze_log_file_and_write_reports(log_path: &Path) -> io::Result<ScanReport> {
    analyze_log_file_and_write_reports_to(log_path, &default_report_dir())
}

pub fn analyze_bundle_dir_and_write_reports(bundle_dir: &Path) -> io::Result<ScanReport> {
    analyze_bundle_dir_and_write_reports_to(bundle_dir, &default_report_dir())
}

pub fn analyze_log_file_and_write_reports_to(
    log_path: &Path,
    output_dir: &Path,
) -> io::Result<ScanReport> {
    let log_text = read_text_lossy(log_path)?;
    let summary = summarize_log(&log_text);
    let findings = analyze_log(&log_text);
    let report = ScanReport {
        xplane_path: log_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        findings,
        plugins: Vec::new(),
        scenery_entries: Vec::new(),
        summary,
    };

    fs::create_dir_all(output_dir)?;
    fs::write(output_dir.join("report.html"), render_html(&report))?;
    fs::write(output_dir.join("report.json"), render_json(&report))?;
    fs::write(
        output_dir.join("forum-summary.txt"),
        render_forum_summary(&report),
    )?;
    Ok(report)
}

pub fn analyze_bundle_dir_and_write_reports_to(
    bundle_dir: &Path,
    output_dir: &Path,
) -> io::Result<ScanReport> {
    let bundle_path = bundle_dir.join("diagnostic-bundle.json");
    let bundle_text = fs::read_to_string(&bundle_path)?;
    let bundle: DiagnosticBundle = serde_json::from_str(&bundle_text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {err}", bundle_path.display()),
        )
    })?;

    let redacted_log_text =
        read_optional_bundle_text(bundle_dir, bundle.log.redacted_file.as_deref());
    let redacted_scenery_ini_text =
        read_optional_bundle_text(bundle_dir, bundle.scenery.redacted_ini_file.as_deref());

    let mut findings = match &redacted_log_text {
        Some(text) => analyze_log(text),
        None => Vec::new(),
    };
    findings.extend(analyze_bundle_input_quality_ascii(
        &bundle,
        redacted_log_text.as_ref(),
        redacted_scenery_ini_text.as_ref(),
    ));
    findings.extend(analyze_bundle_session_status_ascii(&bundle));

    let summary = redacted_log_text
        .as_ref()
        .map(|text| summarize_log(text))
        .unwrap_or_else(|| bundle.log.summary.clone());

    let report = ScanReport {
        xplane_path: bundle_dir.to_path_buf(),
        findings,
        plugins: bundle
            .plugins
            .iter()
            .map(|plugin| plugin.name.clone())
            .collect(),
        scenery_entries: bundle
            .scenery
            .entries
            .iter()
            .map(|entry| SceneryEntry {
                raw: entry.path.clone(),
                path: entry.path.clone(),
                disabled: entry.disabled,
            })
            .collect(),
        summary,
    };

    fs::create_dir_all(output_dir)?;
    fs::write(output_dir.join("report.html"), render_html(&report))?;
    fs::write(output_dir.join("report.json"), render_json(&report))?;
    fs::write(
        output_dir.join("forum-summary.txt"),
        render_forum_summary(&report),
    )?;
    Ok(report)
}

fn scan_xplane(xplane_path: &Path) -> io::Result<ScanReport> {
    let mut findings = Vec::new();

    let log_path = xplane_path.join("Log.txt");
    let log_text = match read_text_lossy(&log_path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            findings.push(Finding::new(
                Severity::High,
                Confidence::High,
                "missing_log",
                tr!("找不到 Log.txt", "Log.txt was not found"),
                log_path.display().to_string(),
                tr!("X-Plane 日志分诊工具需要 X-Plane 生成的最新 Log.txt。", "X-Plane Log Triage Tool needs the latest Log.txt generated by X-Plane."),
                tr!("请确认所选文件夹是 X-Plane 根目录，并且 X-Plane 至少启动过一次。", "Make sure the selected folder is the X-Plane root folder and that X-Plane has been started at least once."),
            ));
            String::new()
        }
        Err(err) => {
            findings.push(Finding::new(
                Severity::High,
                Confidence::High,
                "log_read_failed",
                tr!("Log.txt 存在但无法读取", "Log.txt exists but could not be read"),
                format!("{} ({err})", log_path.display()),
                tr!("文件存在，但操作系统拒绝访问或另一个进程正在使用它。", "The file exists, but the operating system denied access or another process is using it."),
                tr!("关闭 X-Plane 后重试。如果仍然失败，请检查文件权限。", "Close X-Plane and try again. If it still fails, check file permissions."),
            ));
            String::new()
        }
    };

    let summary = summarize_log(&log_text);
    findings.extend(analyze_log(&log_text));

    let plugins = scan_plugins(xplane_path, &mut findings)?;
    let scenery_entries = scan_scenery(xplane_path, &mut findings)?;

    Ok(ScanReport {
        xplane_path: xplane_path.to_path_buf(),
        findings,
        plugins,
        scenery_entries,
        summary,
    })
}

fn scan_plugins(xplane_path: &Path, findings: &mut Vec<Finding>) -> io::Result<Vec<String>> {
    let plugins_path = xplane_path.join("Resources").join("plugins");
    if !plugins_path.exists() {
        findings.push(Finding::new(
            Severity::Low,
            Confidence::High,
            "missing_plugins_dir",
            tr!("找不到 Resources/plugins", "Resources/plugins was not found"),
            plugins_path.display().to_string(),
            tr!("所选文件夹可能不是完整的 X-Plane 根目录，或者安装不完整。", "The selected folder may not be a complete X-Plane root folder, or the installation may be incomplete."),
            tr!("请确认所选文件夹是 X-Plane 根目录。", "Make sure the selected folder is the X-Plane root folder."),
        ));
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    let mut normalized_names: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for entry in fs::read_dir(&plugins_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        normalized_names
            .entry(name.to_ascii_lowercase())
            .or_default()
            .push(name.clone());
        plugins.push(name);
    }

    plugins.sort();

    for names in normalized_names.values() {
        if names.len() > 1 {
            findings.push(Finding::new(
                Severity::Medium,
                Confidence::Medium,
                "duplicate_plugin_name",
                tr!("发现重复的插件文件夹名", "Duplicate plugin folder names were found"),
                names.join(", "),
                tr!("仅大小写不同的插件文件夹名可能造成混淆，尤其在跨平台迁移安装后。", "Plugin folders whose names differ only by case can cause confusion, especially after moving installations across platforms."),
                tr!("只保留实际需要的插件版本，然后重新测试。", "Keep only the plugin version that is actually needed, then retest."),
            ));
        }
    }

    Ok(plugins)
}

fn scan_scenery(xplane_path: &Path, findings: &mut Vec<Finding>) -> io::Result<Vec<SceneryEntry>> {
    let ini_path = xplane_path.join("Custom Scenery").join("scenery_packs.ini");

    if !ini_path.exists() {
        findings.push(Finding::new(
            Severity::Low,
            Confidence::High,
            "missing_scenery_ini",
            tr!("找不到 scenery_packs.ini", "scenery_packs.ini was not found"),
            ini_path.display().to_string(),
            tr!("X-Plane 在启动后创建 scenery_packs.ini。如果缺失，地景顺序可能尚未建立。", "X-Plane creates scenery_packs.ini after startup. If it is missing, scenery order may not be established yet."),
            tr!("启动 X-Plane 一次，关闭后再扫描。", "Start X-Plane once, close it, then scan again."),
        ));
        return Ok(Vec::new());
    }

    let text = fs::read_to_string(&ini_path)?;
    let mut entries = Vec::new();
    let mut active_paths = BTreeMap::<String, usize>::new();

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

        if !disabled && !special_entry {
            *active_paths.entry(path.to_ascii_lowercase()).or_insert(0) += 1;
        }

        let absolute = xplane_path.join(&path);
        if !disabled && !special_entry && !absolute.exists() {
            findings.push(Finding::new(
                Severity::High,
                Confidence::High,
                "missing_scenery_path",
                tr!("scenery_packs.ini 指向不存在的 scenery 文件夹", "scenery_packs.ini points to a missing scenery folder"),
                raw_line.to_string(),
                tr!("一个启用的地景条目指向不存在的文件夹。这可能导致启动警告或加载失败。", "An enabled scenery entry points to a folder that does not exist. This can cause startup warnings or loading failures."),
                tr!("备份 scenery_packs.ini，然后删除无效条目或重新安装缺失的地景包。", "Back up scenery_packs.ini, then remove the stale entry or reinstall the missing scenery package."),
            ));
        }

        entries.push(SceneryEntry {
            raw: raw_line.to_string(),
            path,
            disabled,
        });
    }

    for (path, count) in active_paths {
        if count > 1 {
            findings.push(Finding::new(
                Severity::Medium,
                Confidence::High,
                "duplicate_scenery_entry",
                tr!("scenery_packs.ini 存在重复的启用条目", "scenery_packs.ini has duplicate active entries"),
                format!("{path} appears {count} times"),
                tr!("多次启用同一地景路径通常没有必要，可能增加加载顺序排查的难度。", "Enabling the same scenery path multiple times is usually unnecessary and can make load-order troubleshooting harder."),
                tr!("备份 scenery_packs.ini，然后为同一地景路径只保留一个启用条目。", "Back up scenery_packs.ini, then keep only one active entry for the same scenery path."),
            ));
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scenery_paths_with_spaces() {
        assert_eq!(
            parse_scenery_path("SCENERY_PACK Custom Scenery/KLAX - Los Angeles/"),
            "Custom Scenery/KLAX - Los Angeles/"
        );
        assert_eq!(
            parse_scenery_path("SCENERY_PACK_DISABLED Custom Scenery/Airport Disabled/"),
            "Custom Scenery/Airport Disabled/"
        );
    }

    #[test]
    fn reads_non_utf8_text_lossily() {
        let path = std::env::temp_dir().join("xplane-doctor-non-utf8-log.txt");
        std::fs::write(&path, [b'L', b'o', b'g', 0xff, b'\n']).expect("write temp log");
        let text = read_text_lossy(&path).expect("read lossy text");
        let _ = std::fs::remove_file(&path);
        assert!(text.starts_with("Log"));
    }

    #[test]
    fn extracts_crash_uuids_from_log_markers() {
        let log = "\
0:00:01.000 I/LOG: start
--=={UUID: A04016BB-2A68-4A72-9663-CAF6930F0163}==--
--=={UUID: a04016bb-2a68-4a72-9663-caf6930f0163}==--
";

        assert_eq!(
            extract_crash_uuids(log),
            vec!["a04016bb-2a68-4a72-9663-caf6930f0163"]
        );
        assert!(log_has_crash_marker(log));
    }

    #[test]
    fn matches_current_log_uuid_to_dump_file_name() {
        let root =
            std::env::temp_dir().join(format!("xplane-doctor-crash-uuid-test-{}", unix_now()));
        let reports = root.join("Output").join("crash_reports").join("reports");
        std::fs::create_dir_all(&reports).expect("create reports dir");
        let uuid = "a04016bb-2a68-4a72-9663-caf6930f0163";
        std::fs::write(reports.join(format!("{uuid}.dmp")), b"dmp").expect("write dmp");

        let log_uuid_matches = collect_log_uuid_matches(&[uuid.to_string()], &[]);
        let snapshots =
            collect_crash_report_snapshots(&root, &reports, &log_uuid_matches, None, None)
                .expect("collect crash reports");
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].relation_to_current_log, "uuid_matched");
        assert_eq!(snapshots[0].matched_log_uuid.as_deref(), Some(uuid));
        assert_eq!(
            snapshots[0].matched_log_source.as_deref(),
            Some("current_log")
        );
        assert_eq!(snapshots[0].matched_log_file.as_deref(), Some("Log.txt"));
    }

    #[test]
    fn matches_archive_log_uuid_without_marking_dump_as_current_log() {
        let root = std::env::temp_dir().join(format!(
            "xplane-doctor-archive-crash-uuid-test-{}",
            unix_now()
        ));
        let reports = root.join("Output").join("crash_reports").join("reports");
        std::fs::create_dir_all(&reports).expect("create reports dir");
        let uuid = "6107aa35-7308-4c6e-b5e3-f03b0a9aa342";
        std::fs::write(reports.join(format!("{uuid}.dmp")), b"dmp").expect("write dmp");

        let archives = vec![LogArchiveSnapshot {
            name: "Log-2026-05-04-1653.txt".to_string(),
            kind: "main_log".to_string(),
            size_bytes: 100,
            modified_at_unix_seconds: None,
            clean_shutdown: Some(false),
            crash_uuids: vec![uuid.to_string()],
        }];
        let log_uuid_matches = collect_log_uuid_matches(&[], &archives);
        let snapshots =
            collect_crash_report_snapshots(&root, &reports, &log_uuid_matches, None, None)
                .expect("collect crash reports");
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].relation_to_current_log, "unknown");
        assert_eq!(snapshots[0].matched_log_uuid.as_deref(), Some(uuid));
        assert_eq!(
            snapshots[0].matched_log_source.as_deref(),
            Some("log_archive")
        );
        assert_eq!(
            snapshots[0].matched_log_file.as_deref(),
            Some("Log-2026-05-04-1653.txt")
        );
    }

    #[test]
    fn treats_atc_log_archive_clean_shutdown_as_not_applicable() {
        assert_eq!(log_archive_kind("Log_ATC-2026-05-04-1619.txt"), "atc_log");
        assert_eq!(log_archive_kind("Log-2026-05-04-1653.txt"), "main_log");
    }
}
