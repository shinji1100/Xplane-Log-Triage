use crate::tr;
use crate::tr_fmt;
use crate::{Finding, ScanReport, Severity};

pub(crate) fn render_html(report: &ScanReport) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    html.push_str(&format!(
        "<title>{}</title>",
        tr!(
            "X-Plane 日志分诊工具 报告",
            "X-Plane Log Triage Tool Report"
        )
    ));
    html.push_str("<style>");
    html.push_str(
        "body{font-family:Segoe UI,Arial,sans-serif;margin:0;background:#f6f8fb;color:#18202a}",
    );
    html.push_str("main{max-width:1040px;margin:0 auto;padding:32px 20px}");
    html.push_str("h1{font-size:28px;margin:0 0 8px}h2{font-size:18px;margin:30px 0 10px}h3{font-size:16px;margin:6px 0 8px}");
    html.push_str(
        ".muted{color:#64748b}.summary{display:flex;gap:12px;flex-wrap:wrap;margin:18px 0}",
    );
    html.push_str(
        ".pill{background:white;border:1px solid #d8dee8;border-radius:8px;padding:10px 12px}",
    );
    html.push_str(".finding{background:white;border:1px solid #d8dee8;border-left-width:6px;border-radius:8px;margin:12px 0;padding:14px}");
    html.push_str(".high{border-left-color:#dc2626}.medium{border-left-color:#d97706}.low{border-left-color:#2563eb}.info{border-left-color:#94a3b8}");
    html.push_str(".sev{font-size:12px;font-weight:700;text-transform:uppercase;color:#475569}.evidence{font-family:Consolas,monospace;background:#f1f5f9;padding:8px;border-radius:6px;overflow-wrap:anywhere}");
    html.push_str("ul{padding-left:20px}li{margin:4px 0}.empty{color:#64748b;background:white;border:1px solid #d8dee8;border-radius:8px;padding:14px}");
    html.push_str("</style></head><body><main>");

    html.push_str(&format!(
        "<h1>{}</h1>",
        tr!(
            "X-Plane 日志分诊工具 报告",
            "X-Plane Log Triage Tool Report"
        )
    ));
    html.push_str(&format!(
        "<div class=\"muted\">{} {}</div>",
        tr!("输入：", "Input:"),
        escape_html(&report.xplane_path.display().to_string())
    ));
    html.push_str("<div class=\"summary\">");
    html.push_str(&format!(
        "<div class=\"pill\">{} {}</div>",
        tr!("发现：", "Findings:"),
        report.findings.len()
    ));
    html.push_str(&format!(
        "<div class=\"pill\">{} {}</div>",
        tr!("插件：", "Plugins:"),
        report.plugins.len()
    ));
    html.push_str(&format!(
        "<div class=\"pill\">{} {}</div>",
        tr!("Scenery 条目：", "Scenery entries:"),
        report.scenery_entries.len()
    ));
    html.push_str("</div>");

    render_summary_html(&mut html, report);
    render_finding_group_html(
        &mut html,
        tr!("主要问题", "Main Problems"),
        report,
        |severity| matches!(severity, Severity::High | Severity::Medium),
        tr!(
            "当前规则未发现高或中优先级问题。",
            "No high or medium priority problems were found by the current rules."
        ),
    );
    render_finding_group_html(
        &mut html,
        tr!("建议检查", "Things To Check"),
        report,
        |severity| severity == Severity::Low,
        tr!(
            "未发现低优先级检查项。",
            "No low priority checks were found."
        ),
    );
    render_finding_group_html(
        &mut html,
        tr!("背景 / 技术细节", "Background / Technical Details"),
        report,
        |severity| severity == Severity::Info,
        tr!("未记录背景信息。", "No background notes were recorded."),
    );

    html.push_str(&format!(
        "<h2>{}</h2><ul>",
        tr!("已安装插件", "Installed Plugins")
    ));
    for plugin in &report.plugins {
        html.push_str(&format!("<li>{}</li>", escape_html(plugin)));
    }
    if report.plugins.is_empty() {
        html.push_str(&format!(
            "<li class=\"muted\">{}</li>",
            tr!(
                "未找到或未包含插件目录。",
                "No plugin folders were found or included."
            )
        ));
    }
    html.push_str("</ul>");

    html.push_str("</main></body></html>");
    html
}

pub(crate) fn render_json(report: &ScanReport) -> String {
    serde_json::to_string_pretty(report).expect("ScanReport should always serialize")
}

pub(crate) fn render_forum_summary(report: &ScanReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}\n",
        tr!(
            "X-Plane 日志分诊工具 摘要",
            "X-Plane Log Triage Tool Summary"
        )
    ));
    out.push_str("======================\n\n");
    out.push_str(&format!(
        "{} {}\n\n",
        tr!("输入：", "Input:"),
        anonymize_forum_text(&report.xplane_path.display().to_string())
    ));

    out.push_str(&format!("{}\n", tr!("系统信息：", "System info:")));
    append_summary_line(&mut out, "X-Plane", &report.summary.xplane_version);
    append_summary_line(&mut out, "OS", &report.summary.os);
    append_summary_line(&mut out, "CPU", &report.summary.cpu);
    append_summary_line(&mut out, "GPU", &report.summary.gpu);
    append_summary_line(&mut out, "Aircraft", &report.summary.aircraft);

    out.push_str(&format!("\n{}\n", tr!("主要问题：", "Main problems:")));
    let mut findings = report.findings.clone();
    findings.sort_by_key(|finding| finding.severity);
    let important: Vec<&Finding> = findings
        .iter()
        .filter(|finding| matches!(finding.severity, Severity::High | Severity::Medium))
        .take(8)
        .collect();

    if important.is_empty() {
        out.push_str(&format!(
            "- {}\n",
            tr!(
                "当前规则未发现高或中优先级问题。",
                "No high or medium priority problems were found by the current rules."
            )
        ));
    } else {
        for finding in important {
            out.push_str(&format!(
                "- [{} / {} {}] {}\n  {}: {}\n  {}: {}\n",
                finding.severity.as_str(),
                tr!("置信度", "confidence"),
                finding.confidence.as_str(),
                finding_title_with_count(finding),
                tr!("证据", "Evidence"),
                forum_evidence_lines(finding),
                tr!("建议", "Suggestion"),
                anonymize_forum_text(&finding.suggestion)
            ));
        }
    }

    let background: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Info)
        .take(6)
        .collect();
    if !background.is_empty() {
        out.push_str(&format!("\n{}\n", tr!("背景信息：", "Background notes:")));
        for finding in background {
            out.push_str(&format!(
                "- {}: {}\n",
                finding_title_with_count(finding),
                anonymize_forum_text(&finding.evidence)
            ));
        }
    }

    out.push_str(&format!(
        "\n{}\n",
        tr!("已安装插件目录：", "Installed plugin folders:")
    ));
    if report.plugins.is_empty() {
        out.push_str(&format!(
            "- {}\n",
            tr!("未找到或未包含。", "None found or included.")
        ));
    } else {
        for plugin in report.plugins.iter().take(40) {
            out.push_str(&format!("- {plugin}\n"));
        }
        if report.plugins.len() > 40 {
            out.push_str(&format!(
                "- {}\n",
                tr_fmt!("...另外 {} 个", "...and {} more", report.plugins.len() - 40)
            ));
        }
    }

    out
}

fn render_summary_html(html: &mut String, report: &ScanReport) {
    html.push_str(&format!(
        "<h2>{}</h2><ul>",
        tr!("系统摘要", "System Summary")
    ));
    append_optional_html(html, &report.summary.xplane_version);
    append_optional_html(html, &report.summary.os);
    append_optional_html(html, &report.summary.cpu);
    append_optional_html(html, &report.summary.gpu);
    append_optional_html(html, &report.summary.aircraft);
    html.push_str("</ul>");
}

fn render_finding_group_html(
    html: &mut String,
    title: &str,
    report: &ScanReport,
    include: impl Fn(Severity) -> bool,
    empty_message: &str,
) {
    html.push_str(&format!("<h2>{}</h2>", escape_html(title)));
    let mut findings: Vec<Finding> = report
        .findings
        .iter()
        .filter(|finding| include(finding.severity))
        .cloned()
        .collect();
    findings.sort_by_key(|finding| finding.severity);

    if findings.is_empty() {
        html.push_str(&format!(
            "<p class=\"empty\">{}</p>",
            escape_html(empty_message)
        ));
        return;
    }

    for finding in findings {
        html.push_str(&format!(
            "<section class=\"finding {}\"><div class=\"sev\">{} / confidence {}</div><h3>{}</h3>",
            finding.severity.as_str(),
            finding.severity.as_str(),
            finding.confidence.as_str(),
            escape_html(&finding_title_with_count(&finding))
        ));
        html.push_str(&format!(
            "{}<p>{}</p><p><strong>{}:</strong> {}</p></section>",
            render_evidence_html(&finding),
            escape_html(&finding.explanation),
            tr!("建议", "Suggestion"),
            escape_html(&finding.suggestion)
        ));
    }
}

fn append_optional_html(html: &mut String, value: &Option<String>) {
    if let Some(value) = value {
        html.push_str(&format!("<li>{}</li>", escape_html(value)));
    }
}

fn append_summary_line(out: &mut String, label: &str, value: &Option<String>) {
    if let Some(value) = value {
        out.push_str(&format!("- {label}: {}\n", anonymize_forum_text(value)));
    }
}

fn finding_title_with_count(finding: &Finding) -> String {
    if finding.occurrences > 1 {
        format!("{} x {}", finding.title, finding.occurrences)
    } else {
        finding.title.clone()
    }
}

fn render_evidence_html(finding: &Finding) -> String {
    if finding.extra_evidence.is_empty() {
        return format!(
            "<p class=\"evidence\">{}</p>",
            escape_html(&finding.evidence)
        );
    }

    let mut html = String::new();
    html.push_str("<div class=\"evidence\"><ul>");
    html.push_str(&format!("<li>{}</li>", escape_html(&finding.evidence)));
    for evidence in &finding.extra_evidence {
        html.push_str(&format!("<li>{}</li>", escape_html(evidence)));
    }
    html.push_str("</ul></div>");
    html
}

fn forum_evidence_lines(finding: &Finding) -> String {
    let mut lines = vec![anonymize_forum_text(&finding.evidence)];
    lines.extend(
        finding
            .extra_evidence
            .iter()
            .map(|evidence| anonymize_forum_text(evidence)),
    );

    if finding.occurrences > 1 {
        format!("\n    - {}", lines.join("\n    - "))
    } else {
        lines.into_iter().next().unwrap_or_default()
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn anonymize_forum_text(input: &str) -> String {
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

    anonymize_windows_user_paths(&out)
}

fn anonymize_windows_user_paths(input: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Confidence, SceneryEntry, SystemSummary};
    use std::path::PathBuf;

    fn sample_report() -> ScanReport {
        ScanReport {
            xplane_path: PathBuf::from(r"C:\Users\alice\X-Plane 12"),
            findings: vec![
                Finding {
                    severity: crate::Severity::High,
                    confidence: Confidence::High,
                    kind: "plugin_load_failed".to_string(),
                    title: "Plugin load failed".to_string(),
                    evidence: r"C:\Users\alice\X-Plane 12\Resources\plugins\Test\win.xpl"
                        .to_string(),
                    occurrences: 1,
                    extra_evidence: Vec::new(),
                    explanation: "Test explanation".to_string(),
                    suggestion: "Test suggestion".to_string(),
                },
                Finding {
                    severity: crate::Severity::Info,
                    confidence: Confidence::High,
                    kind: "background".to_string(),
                    title: "Historical background".to_string(),
                    evidence: "old dump".to_string(),
                    occurrences: 1,
                    extra_evidence: Vec::new(),
                    explanation: "Background explanation".to_string(),
                    suggestion: "No immediate action.".to_string(),
                },
            ],
            plugins: vec!["Test".to_string()],
            scenery_entries: vec![SceneryEntry {
                raw: "SCENERY_PACK Custom Scenery/Test/".to_string(),
                path: "Custom Scenery/Test".to_string(),
                disabled: false,
            }],
            summary: SystemSummary::default(),
        }
    }

    #[test]
    fn renders_html_with_grouped_findings() {
        let html = render_html(&sample_report());
        assert!(html.contains("X-Plane Log Triage Tool Report"));
        assert!(html.contains("Main Problems"));
        assert!(html.contains("Background / Technical Details"));
        assert!(html.contains("Plugin load failed"));
    }

    #[test]
    fn renders_json_with_findings() {
        let mut report = sample_report();
        report.findings[0].occurrences = 3;
        report.findings[0]
            .extra_evidence
            .push("Log.txt:2: second evidence".to_string());
        let json = render_json(&report);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("report json should be valid");
        assert_eq!(value["findings"][0]["kind"], "plugin_load_failed");
        assert_eq!(value["findings"][0]["severity"], "high");
        assert_eq!(value["findings"][0]["occurrences"], 3);
        assert_eq!(
            value["findings"][0]["extra_evidence"][0],
            "Log.txt:2: second evidence"
        );
    }

    #[test]
    fn forum_summary_anonymizes_windows_user_paths() {
        let summary = render_forum_summary(&sample_report());
        assert!(!summary.contains("alice"));
        assert!(summary.contains("<user>"));
    }
}
