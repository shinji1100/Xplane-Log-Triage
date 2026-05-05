use std::env;
use std::io;
use std::path::PathBuf;

use xplane_log_triage::{
    analyze_bundle_dir_and_write_reports_to, analyze_log_file_and_write_reports_to,
    choose_detected_install, collect_diagnostic_bundle_to, default_report_dir,
    detect_xplane_installs, i18n::detect_locale, i18n::set_locale, i18n::Locale,
    scan_and_write_reports_to, tr, tr_fmt,
};

fn main() {
    if let Err(err) = run() {
        eprintln!(
            "{}",
            tr_fmt!(
                "X-Plane 日志分诊工具失败：{err}",
                "X-Plane Log Triage Tool failed: {err}"
            )
        );
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let raw_args: Vec<String> = env::args().collect();

    // Detect locale first so error messages are in the right language.
    // --lang flag in parse_common_args can override this.
    set_locale(detect_locale());

    let (args, output_dir) = parse_common_args(raw_args)?;

    if args.len() >= 2 && args[1] == "detect" {
        let candidates = detect_xplane_installs();
        if candidates.is_empty() {
            println!(
                "{}",
                tr!(
                    "没有自动找到 X-Plane 安装目录。",
                    "No X-Plane installation was detected automatically."
                )
            );
        } else {
            println!(
                "{}",
                tr!(
                    "找到以下候选目录：",
                    "Found the following candidate directories:"
                )
            );
            for candidate in candidates {
                println!("  {}", candidate.display());
            }
        }
        return Ok(());
    }

    if args.len() >= 3 && args[1] == "analyze-log" {
        let log_path = PathBuf::from(&args[2]);
        if !log_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                tr_fmt!(
                    "Log.txt 不存在：{}",
                    "Log.txt was not found: {}",
                    log_path.display()
                ),
            ));
        }

        let report = analyze_log_file_and_write_reports_to(&log_path, &output_dir)?;
        print_report_result(report.findings.len(), &output_dir);
        return Ok(());
    }

    if args.len() >= 3 && args[1] == "analyze-bundle" {
        let bundle_dir = PathBuf::from(&args[2]);
        if !bundle_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                tr_fmt!(
                    "诊断包目录不存在：{}",
                    "Bundle directory was not found: {}",
                    bundle_dir.display()
                ),
            ));
        }

        let bundle_path = bundle_dir.join("diagnostic-bundle.json");
        if !bundle_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                tr_fmt!(
                    "没有找到诊断包文件：{}",
                    "Diagnostic bundle file was not found: {}",
                    bundle_path.display()
                ),
            ));
        }

        let report = analyze_bundle_dir_and_write_reports_to(&bundle_dir, &output_dir)?;
        print_report_result(report.findings.len(), &output_dir);
        return Ok(());
    }

    if args.len() >= 2 && args[1] == "collect" {
        let xplane_path = if args.len() >= 3 {
            PathBuf::from(&args[2])
        } else {
            match choose_detected_install() {
                Some(path) => path,
                None => {
                    println!("{}", tr!("没有自动找到 X-Plane 12。请手动指定目录：", "Could not auto-detect X-Plane 12. Please specify the directory manually:"));
                    print_usage(&args);
                    return Ok(());
                }
            }
        };

        if !xplane_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                tr_fmt!(
                    "目录不存在：{}",
                    "Directory was not found: {}",
                    xplane_path.display()
                ),
            ));
        }

        let bundle = collect_diagnostic_bundle_to(&xplane_path, &output_dir)?;
        print_collect_result(&bundle, &output_dir);
        return Ok(());
    }

    if args.len() < 2 || args[1] != "scan" {
        print_usage(&args);
        return Ok(());
    }

    let xplane_path = if args.len() >= 3 {
        PathBuf::from(&args[2])
    } else {
        match choose_detected_install() {
            Some(path) => path,
            None => {
                println!(
                    "{}",
                    tr!(
                        "没有自动找到 X-Plane 12。请手动指定目录：",
                        "Could not auto-detect X-Plane 12. Please specify the directory manually:"
                    )
                );
                print_usage(&args);
                return Ok(());
            }
        }
    };

    if !xplane_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("目录不存在：{}", xplane_path.display()),
        ));
    }

    let report = scan_and_write_reports_to(&xplane_path, &output_dir)?;
    print_report_result(report.findings.len(), &output_dir);
    Ok(())
}

fn parse_common_args(args: Vec<String>) -> io::Result<(Vec<String>, PathBuf)> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut output_dir = default_report_dir();
    let mut iter = args.into_iter();

    if let Some(exe) = iter.next() {
        filtered.push(exe);
    }

    while let Some(arg) = iter.next() {
        if arg == "--output" || arg == "-o" {
            let Some(path) = iter.next() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    tr!(
                        "--output 后面需要跟一个报告目录",
                        "--output requires a report directory"
                    ),
                ));
            };
            output_dir = PathBuf::from(path);
        } else if arg == "--lang" {
            let Some(lang) = iter.next() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    tr!("--lang 需要 zh 或 en", "--lang requires zh or en"),
                ));
            };
            match lang.to_ascii_lowercase().as_str() {
                "zh" | "zh-cn" | "zh-tw" => set_locale(Locale::Zh),
                "en" | "en-us" | "en-gb" => set_locale(Locale::En),
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown language: {other}, expected zh or en"),
                    ))
                }
            }
        } else {
            filtered.push(arg);
        }
    }

    Ok((filtered, output_dir))
}

fn print_report_result(finding_count: usize, output_dir: &std::path::Path) {
    println!("{}", tr!("分析完成：", "Analysis complete:"));
    println!(
        "{} {}",
        tr!("报告目录：", "Report directory:"),
        output_dir.display()
    );
    println!("  {}", output_dir.join("report.html").display());
    println!("  {}", output_dir.join("report.json").display());
    println!("  {}", output_dir.join("forum-summary.txt").display());
    println!(
        "{}",
        tr_fmt!(
            "发现 {finding_count} 条结果。",
            "Found {finding_count} finding(s)."
        )
    );
}

fn print_collect_result(
    bundle: &xplane_log_triage::DiagnosticBundle,
    output_dir: &std::path::Path,
) {
    println!("{}", tr!("诊断包已生成：", "Diagnostic bundle created:"));
    println!("{} {}", tr!("目录：", "Directory:"), output_dir.display());
    println!("  {}", output_dir.join("diagnostic-bundle.json").display());
    if let Some(file) = &bundle.log.redacted_file {
        println!("  {}", output_dir.join(file).display());
    }
    if let Some(file) = &bundle.scenery.redacted_ini_file {
        println!("  {}", output_dir.join(file).display());
    }
    println!(
        "{}",
        tr_fmt!("插件：{} 个", "Plugins: {}", bundle.plugins.len())
    );
    println!(
        "{}",
        tr_fmt!(
            "Scenery 条目：{} 个",
            "Scenery entries: {}",
            bundle.scenery.entries.len()
        )
    );
    println!(
        "{}",
        tr_fmt!(
            "Crash report 元数据：{} 个",
            "Crash report metadata: {}",
            bundle.crash_reports.len()
        )
    );
}

fn print_usage(args: &[String]) {
    let exe = args
        .first()
        .map(String::as_str)
        .unwrap_or("xplane-log-triage");
    println!("{}", tr!("用法：", "Usage:"));
    println!("  {exe} detect");
    println!("  {exe} collect \"D:\\X-Plane 12\"");
    println!("  {exe} scan");
    println!("  {exe} scan \"D:\\X-Plane 12\"");
    println!("  {exe} analyze-log \"D:\\Logs\\Log.txt\"");
    println!("  {exe} analyze-bundle \"D:\\X-Plane Log Triage Reports\"");
    println!("  {exe} scan \"D:\\X-Plane 12\" --output \"D:\\Reports\"");
}
