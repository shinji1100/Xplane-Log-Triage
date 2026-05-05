use std::path::PathBuf;

use crate::tr;
use serde::{Deserialize, Serialize};

// Single source of truth for public types. Do not redefine these elsewhere.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
            Severity::Info => "info",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Severity::High => tr!("严重", "High"),
            Severity::Medium => tr!("注意", "Medium"),
            Severity::Low => tr!("提示", "Low"),
            Severity::Info => tr!("信息", "Info"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub confidence: Confidence,
    pub kind: String,
    pub title: String,
    pub evidence: String,
    pub occurrences: usize,
    pub extra_evidence: Vec<String>,
    pub explanation: String,
    pub suggestion: String,
}

impl Finding {
    pub fn new(
        severity: Severity,
        confidence: Confidence,
        kind: impl Into<String>,
        title: impl Into<String>,
        evidence: impl Into<String>,
        explanation: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            confidence,
            kind: kind.into(),
            title: title.into(),
            evidence: evidence.into(),
            occurrences: 1,
            extra_evidence: Vec::new(),
            explanation: explanation.into(),
            suggestion: suggestion.into(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScanReport {
    pub xplane_path: PathBuf,
    pub findings: Vec<Finding>,
    pub plugins: Vec<String>,
    pub scenery_entries: Vec<SceneryEntry>,
    pub summary: SystemSummary,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Confidence::High => tr!("高", "High"),
            Confidence::Medium => tr!("中", "Medium"),
            Confidence::Low => tr!("低", "Low"),
        }
    }
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct SystemSummary {
    pub xplane_version: Option<String>,
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub gpu: Option<String>,
    pub aircraft: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneryEntry {
    pub raw: String,
    pub path: String,
    pub disabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiagnosticBundle {
    pub manifest: BundleManifest,
    pub root: RootSnapshot,
    pub root_files: Vec<FileSnapshot>,
    pub log: LogSnapshot,
    pub plugins: Vec<PluginSnapshot>,
    pub scenery: ScenerySnapshot,
    pub aircraft: Vec<AircraftSnapshot>,
    pub custom_data_files: Vec<FileSnapshot>,
    pub global_scenery_folders: Vec<String>,
    pub output: OutputSnapshot,
    pub crash_reports: Vec<CrashReportSnapshot>,
    pub crash_aftermath_files: Vec<FileSnapshot>,
    pub privacy: PrivacySnapshot,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BundleManifest {
    pub schema_version: u32,
    #[serde(alias = "doctor_version")]
    pub tool_version: String,
    pub generated_at_unix_seconds: u64,
    pub xplane_path: String,
    pub collection_scope: Vec<String>,
    pub collection_limits: CollectionLimits,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CollectionLimits {
    pub original_dump_files_included: bool,
    pub original_preference_files_included: bool,
    pub max_output_files_per_category: usize,
    pub max_aircraft_file_samples_per_folder: usize,
    pub max_aircraft_plugin_samples_per_folder: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RootSnapshot {
    pub has_xplane_exe: bool,
    pub has_log_txt: bool,
    pub has_aircraft_dir: bool,
    pub has_custom_data_dir: bool,
    pub has_resources_dir: bool,
    pub has_plugins_dir: bool,
    pub has_global_scenery_dir: bool,
    pub has_custom_scenery_dir: bool,
    pub has_scenery_packs_ini: bool,
    pub has_output_dir: bool,
    pub has_crash_reports_dir: bool,
    pub has_crash_report_dumps_dir: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FileSnapshot {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at_unix_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LogSnapshot {
    pub present: bool,
    pub read_error: Option<String>,
    pub redacted_file: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at_unix_seconds: Option<u64>,
    pub line_count: usize,
    pub max_elapsed_seconds: Option<f64>,
    pub inferred_start_unix_seconds: Option<u64>,
    pub clean_shutdown: bool,
    pub crashed: bool,
    pub crash_uuids: Vec<String>,
    pub summary: SystemSummary,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginSnapshot {
    pub name: String,
    pub path: String,
    pub modified_at_unix_seconds: Option<u64>,
    pub platform_dirs: Vec<String>,
    pub xpl_files: Vec<String>,
    pub log_files: Vec<FileSnapshot>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScenerySnapshot {
    pub ini_present: bool,
    pub read_error: Option<String>,
    pub redacted_ini_file: Option<String>,
    pub entries: Vec<SceneryPackSnapshot>,
    pub custom_scenery_folders: Vec<String>,
    pub missing_active_paths: Vec<String>,
    pub duplicate_active_paths: Vec<DuplicatePathSnapshot>,
    pub disabled_count: usize,
    pub special_entries: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SceneryPackSnapshot {
    pub order: usize,
    pub path: String,
    pub disabled: bool,
    pub special_entry: bool,
    pub exists: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DuplicatePathSnapshot {
    pub path: String,
    pub count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CrashReportSnapshot {
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at_unix_seconds: Option<u64>,
    pub relation_to_current_log: String,
    pub matched_log_uuid: Option<String>,
    pub matched_log_source: Option<String>,
    pub matched_log_file: Option<String>,
    pub seconds_from_log_start: Option<i64>,
    pub seconds_from_log_end: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AircraftSnapshot {
    pub name: String,
    pub path: String,
    pub modified_at_unix_seconds: Option<u64>,
    pub acf_file_count: usize,
    pub acf_file_samples: Vec<String>,
    pub plugin_dir_count: usize,
    pub plugin_dir_samples: Vec<String>,
    pub samples_truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OutputSnapshot {
    pub directories: Vec<String>,
    pub log_archive_files: Vec<LogArchiveSnapshot>,
    pub diagnostic_report_files: Vec<FileSnapshot>,
    pub preference_files: Vec<FileSnapshot>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LogArchiveSnapshot {
    pub name: String,
    pub kind: String,
    pub size_bytes: u64,
    pub modified_at_unix_seconds: Option<u64>,
    pub clean_shutdown: Option<bool>,
    pub crash_uuids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PrivacySnapshot {
    pub original_dump_files_included: bool,
    pub redacted_files: Vec<String>,
    pub redaction_rules: Vec<String>,
}
