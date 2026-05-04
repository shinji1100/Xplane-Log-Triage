use std::path::Path;

use crate::collect::read_text_lossy;
use crate::model::*;
use crate::tr;
use crate::tr_fmt;

pub(crate) fn read_optional_bundle_text(
    bundle_dir: &Path,
    file_name: Option<&str>,
) -> Option<String> {
    let file_name = file_name?;
    read_text_lossy(&bundle_dir.join(file_name)).ok()
}

pub(crate) fn analyze_bundle_input_quality_ascii(
    bundle: &DiagnosticBundle,
    redacted_log_text: Option<&String>,
    redacted_scenery_ini_text: Option<&String>,
) -> Vec<Finding> {
    let log_status = if redacted_log_text.is_some() {
        "available"
    } else {
        "missing"
    };
    let scenery_status = if redacted_scenery_ini_text.is_some() {
        "available"
    } else {
        "missing"
    };

    let mut quality = Finding::new(
        Severity::Info,
        Confidence::High,
        "bundle_input_quality",
        tr!("诊断包输入状态", "Diagnostic bundle input status"),
        format!(
            "schema={}, Log.redacted.txt={}, scenery_packs.redacted.ini={}, crash_reports={}, log_archives={}, aftermath={}",
            bundle.manifest.schema_version,
            log_status,
            scenery_status,
            bundle.crash_reports.len(),
            bundle.output.log_archive_files.len(),
            bundle.crash_aftermath_files.len(),
        ),
        tr!("Bundle 分析会使用所有可用的证据文件。缺少脱敏日志或 scenery 文本只会禁用相关规则。", "Bundle analysis uses whatever evidence files are available. Missing redacted logs or scenery text only disables the related rules."),
        tr!("如需最完整的本地分析，请重新运行 collect，并将 diagnostic-bundle.json、Log.redacted.txt 和 scenery_packs.redacted.ini 保留在同一文件夹中。", "For the most complete local analysis, rerun collect and keep diagnostic-bundle.json, Log.redacted.txt, and scenery_packs.redacted.ini in the same folder."),
    );

    if redacted_log_text.is_none() {
        quality.extra_evidence.push(
            tr!(
                "缺少 Log.redacted.txt；已跳过 Log.txt 关键词规则。",
                "Log.redacted.txt is missing; Log.txt keyword rules were skipped."
            )
            .to_string(),
        );
    }
    if redacted_scenery_ini_text.is_none() {
        quality.extra_evidence.push(
            tr!(
                "缺少 scenery_packs.redacted.ini；已跳过 scenery 顺序文本规则。",
                "scenery_packs.redacted.ini is missing; scenery order text rules were skipped."
            )
            .to_string(),
        );
    }

    vec![quality]
}

pub(crate) fn analyze_bundle_session_status_ascii(bundle: &DiagnosticBundle) -> Vec<Finding> {
    let mut findings = Vec::new();
    let current_uuid_matches = bundle
        .crash_reports
        .iter()
        .filter(|report| report.relation_to_current_log == "uuid_matched")
        .count();
    let current_window_reports = bundle
        .crash_reports
        .iter()
        .filter(|report| report.relation_to_current_log == "current_log_window")
        .count();
    let archive_uuid_matches = bundle
        .crash_reports
        .iter()
        .filter(|report| report.matched_log_source.as_deref() == Some("log_archive"))
        .count();
    let before_current_log = bundle
        .crash_reports
        .iter()
        .filter(|report| report.relation_to_current_log == "before_current_log")
        .count();

    if bundle.log.clean_shutdown && !bundle.log.crashed && bundle.log.crash_uuids.is_empty() {
        findings.push(Finding::new(
            Severity::Info,
            Confidence::High,
            "current_session_clean_shutdown",
            tr!("当前 Log.txt 正常退出", "Current Log.txt ended with a clean shutdown"),
            format!(
                "clean_shutdown=true, crashed=false, current_log_crash_uuids={}",
                bundle.log.crash_uuids.len()
            ),
            tr!("当前日志未显示崩溃结尾。除非 Dump 和 Log Archive 条目与当前 Log.txt 匹配，否则它们只是背景证据。", "The current log does not show a crash ending. Dumps and Log Archive entries are background evidence unless they match the current Log.txt."),
            tr!("如果刚刚崩溃，请立即重新采集，并确保包含最新的 Log.txt。", "If the user just crashed, collect again immediately after the crash and make sure the newest Log.txt is included."),
        ));
    } else if bundle.log.crashed || !bundle.log.crash_uuids.is_empty() {
        let mut finding = Finding::new(
            Severity::High,
            Confidence::High,
            "current_session_crashed",
            tr!("当前 Log.txt 显示崩溃", "Current Log.txt shows a crash"),
            format!(
                "crashed={}, current_log_crash_uuids={}",
                bundle.log.crashed,
                bundle.log.crash_uuids.join(", ")
            ),
            tr!("当前日志包含崩溃标记或崩溃 UUID，可作为当前会话的崩溃证据。", "The current log contains a crash marker or crash UUID, so it can be used as current-session crash evidence."),
            tr!("优先关注高置信度的发现以及崩溃前最后加载的插件、飞机、地景或图形错误。", "Prioritize high-confidence findings and the last loaded plugin, aircraft, scenery, or graphics error before the crash."),
        );
        if current_uuid_matches > 0 {
            finding.extra_evidence.push(tr_fmt!(
                "{} 个崩溃 Dump 匹配当前 Log.txt UUID。",
                "{} crash dump(s) matched the current Log.txt UUID.",
                current_uuid_matches
            ));
        }
        findings.push(finding);
    } else {
        findings.push(Finding::new(
            Severity::Info,
            Confidence::Medium,
            "current_session_unknown_exit_state",
            tr!("当前 Log.txt 退出状态不明", "Current Log.txt exit state is unclear"),
            format!(
                "clean_shutdown={}, crashed={}, current_log_crash_uuids={}",
                bundle.log.clean_shutdown,
                bundle.log.crashed,
                bundle.log.crash_uuids.len()
            ),
            tr!("日志既没有正常退出标记也没有崩溃 UUID。可能被截断、仍在使用中或不完整。", "The log has neither a clean-shutdown marker nor a crash UUID. It may be truncated, still in use, or incomplete."),
            tr!("关闭 X-Plane 后重新采集。对于论坛样本，尽量获取完整的 Log.txt。", "Close X-Plane and collect again. For forum samples, ask for the complete Log.txt whenever possible."),
        ));
    }

    if current_uuid_matches == 0 && current_window_reports == 0 && !bundle.crash_reports.is_empty()
    {
        findings.push(Finding::new(
            Severity::Info,
            Confidence::High,
            "historical_crash_reports_only",
            tr!("崩溃 Dump 仅为历史背景", "Crash dumps are historical background only"),
            format!(
                "crash_reports={}, before_current_log={}, archive_uuid_matches={}, current_uuid_matches=0",
                bundle.crash_reports.len(),
                before_current_log,
                archive_uuid_matches
            ),
            tr!("诊断包中包含崩溃 Dump，但没有与当前 Log.txt 匹配的。这说明此机器之前崩溃过，而非当前会话崩溃。", "The bundle contains crash dumps, but none match the current Log.txt. They show that this machine crashed before, not that the current session crashed."),
            tr!("不要将历史 .dmp 文件与正常退出的当前 Log.txt 混为一谈。诊断当前崩溃请在崩溃后立即采集。", "Do not mix historical .dmp files with a clean-shutdown current Log.txt. For current crash diagnosis, collect immediately after the crash."),
        ));
    }

    if archive_uuid_matches > 0 {
        let mut finding = Finding::new(
            Severity::Info,
            Confidence::High,
            "archive_log_crash_uuid_matches",
            tr!("Log Archive 匹配部分崩溃 Dump", "Log Archive matches some crash dumps"),
            tr_fmt!("{} 个崩溃 Dump 匹配 Log Archive UUID。", "{} crash dump(s) matched Log Archive UUIDs.", archive_uuid_matches),
            tr!("这些匹配将历史 Log Archive 文件与历史 Dump 关联起来，可作为背景参考，但不是当前会话证据。", "These matches connect historical Log Archive files to historical dumps. They are useful context, but not current-session evidence."),
            tr!("将这些作为背景趋势证据。当前会话的结论仍应基于当前 Log.txt。", "Use these as background trend evidence. Current-session conclusions should still be based on the current Log.txt."),
        );
        for report in bundle
            .crash_reports
            .iter()
            .filter(|report| report.matched_log_source.as_deref() == Some("log_archive"))
            .take(4)
        {
            finding.extra_evidence.push(format!(
                "{} matched {}",
                report.file_name,
                report.matched_log_file.as_deref().unwrap_or("Log Archive")
            ));
        }
        findings.push(finding);
    }

    if !bundle.crash_aftermath_files.is_empty() {
        findings.push(Finding::new(
            Severity::Info,
            Confidence::Medium,
            "crash_aftermath_present",
            tr!("存在崩溃后续文件", "Crash aftermath file is present"),
            format!(
                "aftermath_files={}",
                bundle
                    .crash_aftermath_files
                    .iter()
                    .map(|file| file.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            tr!("Aftermath 文件通常意味着过去生成了额外的崩溃诊断信息，常见于复杂的图形/设备丢失故障。", "An aftermath file usually means extra crash diagnostics were generated in the past, often around complex graphics/device-loss failures."),
            tr!("此版本仅记录 aftermath 元数据，不读取或打包 zip 内容。除非与当前日志匹配，否则视为背景信息。", "This version records only aftermath metadata and does not read or package zip contents. Treat it as background unless it matches the current log."),
        ));
    }

    findings
}
