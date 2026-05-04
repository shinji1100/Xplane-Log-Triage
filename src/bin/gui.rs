use doctor::{get_locale, set_locale, tr, tr_fmt, Locale};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::process::Command;
use xplane_doctor as doctor;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 680.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "X-Plane Log Triage Tool",
        options,
        Box::new(|cc| {
            configure_fonts(&cc.egui_ctx);
            Ok(Box::<DoctorApp>::default())
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) {
    let Some((font_name, font_bytes)) = load_cjk_font() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        font_name.clone(),
        egui::FontData::from_owned(font_bytes).into(),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, font_name.clone());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, font_name);

    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<(String, Vec<u8>)> {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ];

    for candidate in candidates {
        if let Ok(bytes) = std::fs::read(candidate) {
            return Some(("cjk_system_font".to_string(), bytes));
        }
    }

    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanMode {
    Folder,
    LogFile,
}

struct DoctorApp {
    mode: ScanMode,
    xplane_path: String,
    log_path: String,
    output_dir: PathBuf,
    status: String,
    findings: Vec<doctor::Finding>,
    summary: doctor::SystemSummary,
    plugin_count: usize,
    scenery_count: usize,
    detected_paths: Vec<PathBuf>,
    report_ready: bool,
    locale: Locale,
}

impl Default for DoctorApp {
    fn default() -> Self {
        let detected_paths = doctor::detect_xplane_installs();
        let xplane_path = detected_paths
            .first()
            .map(|path| path.display().to_string())
            .unwrap_or_default();

        let locale = get_locale();
        let status = if detected_paths.is_empty() {
            tr!("没有自动找到 X-Plane 12。可以手动选择安装目录，或改为选择一个 Log.txt。", "No X-Plane 12 installation was detected automatically. You can manually select the folder, or switch to analyzing a Log.txt.").to_string()
        } else {
            tr_fmt!(
                "自动找到 {} 个候选目录，已填入第一个。",
                "Auto-detected {} candidate(s), filled the first one.",
                detected_paths.len()
            )
        };

        Self {
            mode: ScanMode::Folder,
            xplane_path,
            log_path: String::new(),
            output_dir: doctor::default_report_dir(),
            status,
            findings: Vec::new(),
            summary: doctor::SystemSummary::default(),
            plugin_count: 0,
            scenery_count: 0,
            detected_paths,
            report_ready: false,
            locale,
        }
    }
}

impl eframe::App for DoctorApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Language toggle + heading
            ui.horizontal(|ui| {
                ui.heading("X-Plane Log Triage Tool");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(self.locale == Locale::Zh, "中文")
                        .clicked()
                    {
                        self.locale = Locale::Zh;
                        set_locale(Locale::Zh);
                    }
                    if ui
                        .selectable_label(self.locale == Locale::En, "English")
                        .clicked()
                    {
                        self.locale = Locale::En;
                        set_locale(Locale::En);
                    }
                });
            });
            ui.label(tr!("本地检查 X-Plane 12 的日志、插件和 scenery 配置，并生成可分享的诊断报告。", "Locally inspect X-Plane 12 logs, plugins, and scenery configuration, and generate shareable diagnostic reports."));
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, ScanMode::Folder, tr!("扫描 X-Plane 文件夹", "Scan X-Plane Folder"));
                ui.selectable_value(&mut self.mode, ScanMode::LogFile, tr!("只分析 Log.txt", "Analyze Log.txt Only"));
            });

            ui.add_space(10.0);
            match self.mode {
                ScanMode::Folder => self.render_folder_picker(ui),
                ScanMode::LogFile => self.render_log_picker(ui),
            }

            ui.add_space(8.0);
            self.render_output_picker(ui);

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let action = match self.mode {
                    ScanMode::Folder => tr!("开始扫描", "Start Scan"),
                    ScanMode::LogFile => tr!("分析日志", "Analyze Log"),
                };

                if ui.button(action).clicked() {
                    self.run_diagnosis();
                }

                if ui
                    .add_enabled(self.report_ready, egui::Button::new(tr!("打开报告", "Open Report")))
                    .clicked()
                {
                    open_path(&self.output_dir.join("report.html"));
                }

                if ui
                    .add_enabled(self.report_ready, egui::Button::new(tr!("打开报告文件夹", "Open Report Folder")))
                    .clicked()
                {
                    open_path(&self.output_dir);
                }
            });

            ui.add_space(8.0);
            ui.label(&self.status);

            ui.separator();
            self.render_summary(ui);

            ui.separator();
            ui.heading(tr!("诊断结果", "Diagnostic Results"));
            if self.findings.is_empty() {
                ui.label(tr!("还没有诊断结果。", "No diagnostic results yet."));
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut findings: Vec<&doctor::Finding> = self.findings.iter().collect();
                findings.sort_by_key(|finding| finding.severity);

                for finding in findings {
                    render_finding(ui, finding);
                    ui.add_space(8.0);
                }
            });
        });
    }
}

impl DoctorApp {
    fn render_folder_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(tr!("X-Plane 12 目录", "X-Plane 12 Directory"));
            ui.text_edit_singleline(&mut self.xplane_path);

            if ui.button(tr!("选择", "Choose")).clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.xplane_path = path.display().to_string();
                }
            }

            if ui.button(tr!("自动查找", "Auto-detect")).clicked() {
                self.detected_paths = doctor::detect_xplane_installs();
                if let Some(path) = self.detected_paths.first() {
                    self.xplane_path = path.display().to_string();
                    self.status = tr_fmt!(
                        "已选择第一个候选目录，共找到 {} 个。",
                        "Selected the first candidate, {} found in total.",
                        self.detected_paths.len()
                    );
                } else {
                    self.status = tr!(
                        "没有自动找到 X-Plane 12。",
                        "No X-Plane 12 installation was detected."
                    )
                    .to_string();
                }
            }
        });

        if !self.detected_paths.is_empty() {
            egui::CollapsingHeader::new(tr!("候选目录", "Candidates"))
                .default_open(false)
                .show(ui, |ui| {
                    for path in &self.detected_paths {
                        if ui.button(path.display().to_string()).clicked() {
                            self.xplane_path = path.display().to_string();
                        }
                    }
                });
        }
    }

    fn render_log_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Log.txt");
            ui.text_edit_singleline(&mut self.log_path);

            if ui.button(tr!("选择", "Choose")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("X-Plane Log", &["txt"])
                    .pick_file()
                {
                    self.log_path = path.display().to_string();
                }
            }
        });
    }

    fn render_output_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(tr!("报告目录", "Report Directory"));
            let mut output = self.output_dir.display().to_string();
            if ui.text_edit_singleline(&mut output).changed() {
                self.output_dir = PathBuf::from(output.trim());
            }

            if ui.button(tr!("选择", "Choose")).clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.output_dir = path;
                }
            }
        });
    }

    fn render_summary(&self, ui: &mut egui::Ui) {
        ui.heading(tr!("摘要", "Summary"));
        ui.horizontal_wrapped(|ui| {
            ui.label(tr_fmt!("发现：{} 条", "Findings: {}", self.findings.len()));
            ui.label(tr_fmt!(
                "插件目录：{} 个",
                "Plugin folders: {}",
                self.plugin_count
            ));
            ui.label(tr_fmt!(
                "Scenery 条目：{} 个",
                "Scenery entries: {}",
                self.scenery_count
            ));
        });

        render_optional_summary(ui, "X-Plane", &self.summary.xplane_version);
        render_optional_summary(ui, "CPU", &self.summary.cpu);
        render_optional_summary(ui, "GPU", &self.summary.gpu);
        render_optional_summary(ui, tr!("系统", "OS"), &self.summary.os);
        render_optional_summary(ui, tr!("飞机", "Aircraft"), &self.summary.aircraft);
    }

    fn run_diagnosis(&mut self) {
        self.report_ready = false;

        let result = match self.mode {
            ScanMode::Folder => {
                let path = PathBuf::from(self.xplane_path.trim());
                if self.xplane_path.trim().is_empty() {
                    self.status = tr!(
                        "请先选择 X-Plane 12 目录。",
                        "Please select an X-Plane 12 folder first."
                    )
                    .to_string();
                    return;
                }
                if !path.exists() {
                    self.status = tr!(
                        "这个目录不存在，请重新选择。",
                        "This folder does not exist. Please select another."
                    )
                    .to_string();
                    return;
                }
                doctor::scan_and_write_reports_to(&path, &self.output_dir)
            }
            ScanMode::LogFile => {
                let path = PathBuf::from(self.log_path.trim());
                if self.log_path.trim().is_empty() {
                    self.status =
                        tr!("请先选择 Log.txt。", "Please select a Log.txt first.").to_string();
                    return;
                }
                if !path.exists() {
                    self.status = tr!(
                        "这个 Log.txt 不存在，请重新选择。",
                        "This Log.txt does not exist. Please select another."
                    )
                    .to_string();
                    return;
                }
                doctor::analyze_log_file_and_write_reports_to(&path, &self.output_dir)
            }
        };

        match result {
            Ok(report) => self.apply_report(report),
            Err(err) => {
                self.status = tr_fmt!("诊断失败：{err}", "Diagnosis failed: {err}");
                self.findings.clear();
                self.plugin_count = 0;
                self.scenery_count = 0;
            }
        }
    }

    fn apply_report(&mut self, report: doctor::ScanReport) {
        let count = report.findings.len();
        self.plugin_count = report.plugins.len();
        self.scenery_count = report.scenery_entries.len();
        self.summary = report.summary;
        self.findings = report.findings;
        self.report_ready = true;
        self.status = tr_fmt!(
            "诊断完成，发现 {count} 条结果。报告目录：{}",
            "Diagnosis complete. Found {count} finding(s). Report directory: {}",
            self.output_dir.display()
        );
    }
}

fn render_optional_summary(ui: &mut egui::Ui, label: &str, value: &Option<String>) {
    if let Some(value) = value {
        ui.label(tr_fmt!("{label}：{value}", "{label}: {value}"));
    }
}

fn render_finding(ui: &mut egui::Ui, finding: &doctor::Finding) {
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                severity_color(finding.severity),
                format!(
                    "{} / {}{}",
                    finding.severity.label(),
                    tr!("置信度", "confidence "),
                    finding.confidence.label()
                ),
            );
            ui.strong(finding_title_with_count(finding));
        });

        ui.label(&finding.explanation);
        ui.label(format!(
            "{}{}",
            tr!("建议：", "Suggestion: "),
            finding.suggestion
        ));
        ui.monospace(format!(
            "{}{}",
            tr!("证据：", "Evidence: "),
            finding.evidence
        ));

        for evidence in &finding.extra_evidence {
            ui.monospace(format!("      {evidence}"));
        }
    });
}

fn finding_title_with_count(finding: &doctor::Finding) -> String {
    if finding.occurrences > 1 {
        tr_fmt!("{} × {} 次", "{} x {}", finding.title, finding.occurrences)
    } else {
        finding.title.clone()
    }
}

fn severity_color(severity: doctor::Severity) -> egui::Color32 {
    match severity {
        doctor::Severity::High => egui::Color32::from_rgb(190, 18, 60),
        doctor::Severity::Medium => egui::Color32::from_rgb(180, 83, 9),
        doctor::Severity::Low => egui::Color32::from_rgb(37, 99, 235),
        doctor::Severity::Info => egui::Color32::from_rgb(71, 85, 105),
    }
}

fn open_path(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(path)
            .spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(path).spawn();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}
