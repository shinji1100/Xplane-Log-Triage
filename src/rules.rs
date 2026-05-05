use std::collections::BTreeMap;

use crate::tr;
use crate::tr_fmt;
use crate::{Confidence, Finding, Severity};

const EARLY_CRASH_SIM_SECONDS: f64 = 5.0;
const ABRUPT_TERMINATION_MIN_SIM_SECONDS: f64 = 300.0;
const MAX_EXTRA_EVIDENCE: usize = 4;

// ── per-line rule checks ────────────────────────────────────────────

fn check_plugin_load_failed(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("dlerror")
        || lower.contains("error code = 126")
        || lower.contains("not a valid win32 application")
        || lower.contains("failed to load")
    {
        Some(Finding::new(
            Severity::High,
            Confidence::High,
            "plugin_load_failed",
            tr!("插件或动态库加载失败", "Plugin or library failed to load"),
            evidence_line(idx, line),
            tr!("X-Plane 明确记录了某个插件或依赖库没有成功加载。常见原因是插件版本不兼容、缺少运行库，或文件安装不完整。", "X-Plane logged that a plugin or required library did not load. Common causes: incompatible plugin version, missing runtime libraries, or incomplete file installation."),
            tr!("先确认该插件支持你的 X-Plane 版本和系统平台；Windows 用户也可以检查 Visual C++ Runtime 是否完整。", "Verify the plugin supports your X-Plane version and platform. Windows users should also check that Visual C++ Runtime is installed."),
        ))
    } else {
        None
    }
}

fn check_vulkan_result_error(idx: usize, line: &str, vr: Option<&str>) -> Option<Finding> {
    let code = vr?;
    Some(Finding::new(
        vulkan_result_severity(code),
        Confidence::High,
        "vulkan_result_error",
        tr_fmt!("Vulkan 官方错误码：{code}", "Vulkan error code: {code}"),
        evidence_line(idx, line),
        vulkan_result_explanation(code),
        vulkan_result_suggestion(code),
    ))
}

fn check_graphics_api_error(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if !is_graphics_api_error_line(lower) {
        return None;
    }
    Some(Finding::new(
        Severity::Medium,
        Confidence::Medium,
        "graphics_vulkan_error",
        tr!("Vulkan/图形接口相关错误", "Vulkan/graphics API related error"),
        evidence_line(idx, line),
        tr!("日志里出现了 Vulkan 相关错误，可能和显卡驱动、图形设置或插件绘制有关。", "Vulkan-related errors appeared in the log. Possible causes: GPU driver issues, graphics settings, or plugin rendering."),
        tr!("更新显卡驱动，临时关闭图形增强类插件，再用默认飞机和默认机场复测。", "Update GPU drivers, temporarily disable graphics-enhancement plugins, and retest with default aircraft at a default airport."),
    ))
}

fn check_texture_vram_pressure(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if !lower.contains("target scale moved to") || !lower.contains("texture usage") {
        return None;
    }
    let (usage_mb, usage_str) = parse_after_key(line, "Texture usage is ")?;
    let (avail_mb, _avail_str) = parse_after_key(line.split(" out of ").nth(1)?, "")?;
    let (headroom_mb, headroom_str) = parse_after_key(line, "Memory headroom is ")?;

    let over_significant = usage_mb > avail_mb * 1.1;
    let over_mild = usage_mb > avail_mb && !over_significant;
    let headroom_low = headroom_mb < 100.0;
    let headroom_moderate = headroom_mb < 1000.0;

    if !over_significant && !over_mild && !headroom_moderate {
        return None;
    }

    let (severity, title) = if over_significant {
        let pct = ((usage_mb - avail_mb) / avail_mb * 100.0) as u32;
        (
            Severity::High,
            tr_fmt!("显存严重超限——纹理使用{usage_str}，超出预算{_avail_str}约{pct}%", "VRAM severely over-committed -- texture usage {usage_str}, exceeds budget {_avail_str} by ~{pct}%"),
        )
    } else if over_mild || headroom_low {
        (
            Severity::Medium,
            tr_fmt!("显存压力——纹理使用{usage_str}，可用{_avail_str}，余量{headroom_str}", "VRAM pressure -- texture usage {usage_str}, available {_avail_str}, headroom {headroom_str}"),
        )
    } else {
        (
            Severity::Low,
            tr_fmt!(
                "显存余量偏低——{headroom_str}",
                "VRAM headroom low -- {headroom_str}"
            ),
        )
    };

    let target_scale = parse_target_scale(line);
    let scale_extreme = target_scale.map_or(false, |s| s <= 0.3);
    let (severity, title) = if scale_extreme && severity == Severity::Medium {
        let s = target_scale.unwrap_or(0.0);
        (
            Severity::High,
            tr_fmt!(
                "显存严重不足，纹理大幅降级至 {s:.2}x——{usage_str}使用中，可用{_avail_str}",
                "VRAM critically low, extreme texture downscale to {s:.2}x -- usage {usage_str}, available {_avail_str}"
            ),
        )
    } else {
        (severity, title)
    };

    Some(Finding::new(
        severity,
        Confidence::High,
        "texture_vram_pressure",
        title,
        evidence_line(idx, line),
        tr!("X-Plane 的纹理系统检测到严重显存压力。当纹理使用量超过可用显存时，X-Plane 会频繁降低纹理分辨率，最严重时 GPU 驱动可能直接终止进程（TDR），导致日志无崩溃标记就突然截断。", "X-Plane's texture system detected significant VRAM pressure. When usage exceeds capacity, textures are downscaled repeatedly, and in worst cases the GPU driver may terminate the process (TDR), causing the log to truncate without a crash marker."),
        tr!("降低纹理质量、抗锯齿；减少高分辨率 scenery 和 ortho；关闭不必要插件后复测同一路线。", "Lower texture quality and anti-aliasing; reduce high-resolution scenery and ortho; disable unnecessary plugins and retest the same route."),
    ))
}

fn parse_after_key<'a>(haystack: &'a str, key: &str) -> Option<(f64, String)> {
    let after = if key.is_empty() {
        haystack
    } else {
        haystack.split(key).nth(1)?
    };
    let value: f64 = after.trim().split_whitespace().next()?.parse().ok()?;
    let unit = after.trim().split_whitespace().nth(1)?.to_ascii_lowercase();
    let mb = if unit.starts_with("gb") {
        value * 1024.0
    } else {
        value
    };
    Some((mb, format!("{value:.2}{unit}")))
}

fn parse_target_scale(line: &str) -> Option<f64> {
    let rest = line.split("Target scale moved to ").nth(1)?;
    let num_str = rest.trim().split_whitespace().next()?;
    num_str.trim_end_matches('.').parse().ok()
}

fn check_memory_vram(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("vram") || lower.contains("out of memory") {
        Some(Finding::new(
            Severity::Medium,
            Confidence::Medium,
            "memory_or_vram",
            tr!("显存或内存压力线索", "VRAM or memory pressure indicator"),
            evidence_line(idx, line),
            tr!("日志里出现显存、内存或资源不足相关字样。这可能导致卡顿、贴图异常或崩溃。", "Terms related to VRAM, memory, or resource exhaustion appeared. These can cause stutters, texture anomalies, or crashes."),
            tr!("降低纹理质量和抗锯齿，减少高分辨率 scenery，复测同一路线。", "Lower texture quality and anti-aliasing, reduce high-resolution scenery, and retest the same route."),
        ))
    } else {
        None
    }
}

fn check_missing_scenery_asset(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if !lower.contains("the art asset") || !lower.contains("could not be found") {
        return None;
    }
    let missing_library = extract_quoted_asset(line)
        .and_then(|asset| asset.split('/').next().map(str::to_string))
        .filter(|name| !name.is_empty());
    let library_hint = missing_library
        .as_ref()
        .map(|name| tr_fmt!(" 日志里的缺失路径以 `{name}` 开头，这通常就是缺少的库或资源包线索。", " The missing path starts with `{name}`, which is usually the library or resource package that needs to be installed."))
        .unwrap_or_default();
    Some(Finding::new(
        Severity::High,
        Confidence::High,
        "missing_scenery_asset",
        tr!("风景包缺少资源文件", "Scenery package missing art asset"),
        evidence_line(idx, line),
        tr_fmt!("某个机场或风景包引用了不存在的对象、贴图或库文件。{library_hint}", "An airport or scenery package referenced a non-existent object, texture, or library file. {library_hint}"),
        tr!("检查该 scenery 的说明文档，安装缺失的 library；如果已经安装，确认库文件夹名称和位置在 Custom Scenery 下。", "Check the scenery documentation and install the missing library. If already installed, verify the library folder name and location under Custom Scenery."),
    ))
}

fn check_missing_scenery_object(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if !lower.contains("unable to locate object")
        && !(lower.contains("missing object") && lower.contains("from package"))
    {
        return None;
    }
    let asset = if lower.contains("unable to locate object") {
        line.split("Unable to locate object:")
            .nth(1)
            .map(str::trim)
            .unwrap_or("")
    } else {
        line.split("Missing object ")
            .nth(1)
            .and_then(|rest| rest.split(" from package").next())
            .map(str::trim)
            .unwrap_or("")
    };
    let library_hint = asset
        .split('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(|name| tr_fmt!(" 缺失对象路径以 `{name}` 开头，优先检查这个库或资源包。", " The missing object path starts with `{name}`; check this library or resource package first."))
        .unwrap_or_default();
    Some(Finding::new(
        Severity::High,
        Confidence::High,
        "missing_scenery_object",
        tr!("风景包找不到对象文件", "Scenery package cannot locate object file"),
        evidence_line(idx, line),
        tr_fmt!("机场或 scenery 正在引用一个不存在的对象文件。{library_hint}", "An airport or scenery is referencing a non-existent object file. {library_hint}"),
        tr!("安装或修复对应 scenery library，然后重新启动 X-Plane 测试同一机场。", "Install or repair the corresponding scenery library, then restart X-Plane and test the same airport."),
    ))
}

fn check_missing_scenery_library(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("requires an additional library scenery package")
        || lower.contains("additional library scenery package that is not installed")
    {
        Some(Finding::new(
            Severity::High,
            Confidence::High,
            "missing_scenery_library",
            tr!("缺少 scenery 依赖库", "Missing scenery dependency library"),
            evidence_line(idx, line),
            tr!("X-Plane 明确提示某个 scenery 需要额外的 library，但当前没有安装或文件不完整。", "X-Plane explicitly reports that a scenery requires an additional library package that is not installed or has incomplete files."),
            tr!("查看该机场/风景包页面的依赖列表，安装所有 required libraries；再确认它们位于 Custom Scenery 目录。", "Check the dependency list on the airport/scenery download page, install all required libraries, and confirm they are in the Custom Scenery directory."),
        ))
    } else {
        None
    }
}

fn check_missing_global_scenery_tile(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("warning for missing scenery file") {
        Some(Finding::new(
            Severity::Medium,
            Confidence::High,
            "missing_global_scenery_tile",
            tr!("缺少全球地景区域块", "Missing global scenery tile"),
            evidence_line(idx, line),
            tr!("X-Plane 提示某个经纬度区域的基础地景文件缺失，这通常不是机场库问题，而是 global scenery 没装全或安装损坏。", "X-Plane reports that a base scenery file for specific coordinates is missing. This is usually not an airport library problem; it indicates global scenery is not fully installed or is corrupted."),
            tr!("运行 X-Plane Installer，选择更新/添加全球地景，确认对应区域已安装。", "Run the X-Plane Installer, choose Update/Add Global Scenery, and verify the corresponding region is installed."),
        ))
    } else {
        None
    }
}

fn check_vulkan_device_lost(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("vulkan device loss") || lower.contains("device lost") {
        Some(Finding::new(
            Severity::High,
            Confidence::High,
            "vulkan_device_lost",
            tr!("Vulkan device lost 崩溃", "Vulkan device lost crash"),
            evidence_line(idx, line),
            tr!("这是图形设备丢失类崩溃，常见关联因素包括显卡驱动、超频/不稳定、显存压力、图形增强插件或特定版本的 X-Plane 图形问题。", "This is a graphics device loss crash. Common associated factors: GPU driver issues, overclocking/instability, VRAM pressure, graphics-enhancement plugins, or X-Plane graphics issues with a specific version."),
            tr!("先更新或回退显卡驱动，关闭超频和图形增强插件，用默认飞机+默认机场复测；如果只在某机场发生，再检查该 scenery。", "Update or roll back GPU drivers, disable overclocking and graphics-enhancement plugins, retest with default aircraft at a default airport. If it only happens at a specific airport, investigate that scenery."),
        ))
    } else {
        None
    }
}

fn sanitize_xplm_bypass_plugin_name(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name).trim()
}

fn check_xplm_bypass(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if !lower.contains("bypassed xplm") && !lower.contains("bypassed sdk when calling") {
        return None;
    }
    let plugin_name = line
        .split(" has bypassed")
        .next()
        .map(sanitize_xplm_bypass_plugin_name)
        .unwrap_or("");
    let title = if plugin_name.is_empty() {
        tr!("插件绕过 XPLM SDK 调用", "Plugin bypassed XPLM SDK call").to_string()
    } else {
        tr_fmt!(
            "插件绕过 XPLM SDK 调用——{plugin_name}",
            "Plugin bypassed XPLM SDK call -- {plugin_name}"
        )
    };
    let suggestion = if plugin_name.is_empty() {
        tr!("定位绕过 XPLM 的插件，更新到最新版或临时禁用复测。", "Identify the plugin that bypassed XPLM, update it to the latest version, or temporarily disable it and retest.").to_string()
    } else {
        tr_fmt!("{plugin_name} 存在 SDK 调用问题。先更新该插件到最新版，若问题依旧则临时禁用它复测。", "{plugin_name} has an SDK calling issue. Update to the latest version; if the problem persists, temporarily disable it and retest.")
    };
    Some(Finding::new(
        Severity::High,
        Confidence::High,
        "xplm_bypass",
        title,
        evidence_line(idx, line),
        tr!("某插件绕过 XPLM 直接调用 SDK 函数，这与线程违规类似，可能导致不可预测的行为或崩溃。", "A plugin bypassed XPLM and called SDK functions directly. This is similar to a threading violation and can cause unpredictable behavior or crashes."),
        suggestion,
    ))
}

fn check_threading_violation(
    idx: usize,
    line: &str,
    lower: &str,
    lines: &[&str],
) -> Option<Finding> {
    if !lower.contains("threading violation calling xplm") {
        return None;
    }
    let plugin_name = lines
        .get(idx + 1)
        .and_then(|next| next.strip_prefix("Violation by "))
        .map(str::trim)
        .unwrap_or("");
    let title = if plugin_name.is_empty() {
        tr!("插件线程违规", "Plugin threading violation").to_string()
    } else {
        tr_fmt!(
            "插件线程违规——{plugin_name}",
            "Plugin threading violation -- {plugin_name}"
        )
    };
    let suggestion = if plugin_name.is_empty() {
        tr!(
            "先定位违规插件，然后更新该插件或将其临时禁用复测。",
            "Identify the violating plugin, then update it or temporarily disable it and retest."
        )
        .to_string()
    } else {
        tr_fmt!("{plugin_name} 存在线程安全问题。先更新该插件到最新版，若问题依旧则临时禁用它复测。", "{plugin_name} has a threading safety issue. Update to the latest version; if the problem persists, temporarily disable it and retest.")
    };
    Some(Finding::new(
        Severity::High,
        Confidence::High,
        "threading_violation",
        title,
        evidence_line(idx, line),
        tr!("某插件在非主线程中调用了 X-Plane SDK 函数，这会导致不可预测的行为甚至崩溃。", "A plugin called X-Plane SDK functions from a non-main thread, causing unpredictable behavior or crashes."),
        suggestion,
    ))
}

fn check_flywithlua_error(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("flywithlua") && (lower.contains("error") || lower.contains("stopped")) {
        Some(Finding::new(
            Severity::Medium,
            Confidence::Medium,
            "flywithlua_error",
            tr!("FlyWithLua 脚本错误线索", "FlyWithLua script error detected"),
            evidence_line(idx, line),
            tr!("FlyWithLua 或某个 Lua 脚本报告了错误。脚本错误可能导致功能失效、卡顿，甚至影响其他插件。", "FlyWithLua or a Lua script reported an error. Script errors can cause feature failures, stutters, or affect other plugins."),
            tr!("临时移走 FlyWithLua/Scripts 里的第三方脚本，只保留 FlyWithLua 本体复测，再逐个放回。", "Temporarily remove third-party scripts from FlyWithLua/Scripts, keep only FlyWithLua itself for a retest, then add them back one by one."),
        ))
    } else {
        None
    }
}

fn check_third_party_plugin_error(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if !is_third_party_plugin_error_line(line, lower) {
        return None;
    }

    let plugin_name = extract_plugin_log_name(line).unwrap_or("third-party plugin");
    Some(Finding::new(
        Severity::Low,
        Confidence::Medium,
        "third_party_plugin_error",
        tr_fmt!(
            "第三方插件报告错误：{plugin_name}",
            "Third-party plugin reported an error: {plugin_name}"
        ),
        evidence_line(idx, line),
        tr!(
            "第三方插件在 X-Plane 标准的 E/... 通道之外写入了自己的 ERROR 消息。这通常指向插件功能问题，而不是模拟器崩溃的直接原因。",
            "A third-party plugin wrote its own ERROR message outside X-Plane's normal E/... channels. This usually points to a plugin feature problem rather than proving the whole simulator crash cause."
        ),
        tr_fmt!(
            "更新或临时禁用 {plugin_name}，如果用户症状与该插件功能匹配则复测。将其视为辅助证据，除非紧邻崩溃前出现。",
            "Update or temporarily disable {plugin_name} and retest if the user's symptom matches this plugin's feature. Treat it as supporting evidence unless it appears immediately before the crash."
        ),
    ))
}

fn is_third_party_plugin_error_line(line: &str, lower: &str) -> bool {
    if lower.contains("flywithlua") {
        return false;
    }
    if parse_xplane_error_channel(line).is_some() {
        return false;
    }

    let has_plugin_tag = extract_plugin_log_name(line).is_some();
    let has_error_word =
        line.contains(" ERROR") || line.contains("[ERROR]") || line.contains("|ERROR|");

    has_plugin_tag && has_error_word || is_plugin_timeout_line(line)
}

fn extract_plugin_log_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if let Some(name) = extract_bracketed_plugin_name(trimmed) {
        return Some(name);
    }

    extract_timed_plugin_prefix(trimmed)
}

fn extract_bracketed_plugin_name(line: &str) -> Option<&str> {
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        let end = rest.find(']')?;
        let name = rest[..end].trim();
        if is_plausible_plugin_log_name(name) {
            return Some(name);
        }
        rest = &rest[end + 1..];
    }

    None
}

fn extract_timed_plugin_prefix(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    let name = parts.next()?;
    let time = parts.next()?;
    if is_plausible_plugin_log_name(name) && looks_like_clock_time(time) {
        Some(name)
    } else {
        None
    }
}

fn is_plugin_timeout_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    extract_timed_plugin_prefix(trimmed).is_some() && trimmed.contains("Request timed out")
}

fn is_plausible_plugin_log_name(name: &str) -> bool {
    if name.is_empty()
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "error" | "warn" | "warning" | "info" | "debug" | "trace"
        )
    {
        return false;
    }

    name.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ' '))
}

fn looks_like_clock_time(value: &str) -> bool {
    let mut parts = value.split(':');
    let Some(hours) = parts.next() else {
        return false;
    };
    let Some(minutes) = parts.next() else {
        return false;
    };
    let Some(seconds) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !hours.is_empty()
        && !minutes.is_empty()
        && !seconds.is_empty()
        && hours.chars().all(|ch| ch.is_ascii_digit())
        && minutes.chars().all(|ch| ch.is_ascii_digit())
        && seconds.chars().all(|ch| ch.is_ascii_digit())
}

fn check_macos_security_block(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    let is_flywithlua_quarantine =
        lower.contains("flywithlua") && lower.contains("scripts quarantine dir");
    if is_flywithlua_quarantine {
        return None;
    }
    if lower.contains("notarized")
        || lower.contains("code signature")
        || lower.contains("quarantine")
    {
        Some(Finding::new(
            Severity::Medium,
            Confidence::Medium,
            "macos_security_block",
            tr!("macOS 安全限制可能阻止插件加载", "macOS security policy may block plugin loading"),
            evidence_line(idx, line),
            tr!("日志出现 quarantine、notarized 或 code signature 相关字样，macOS 可能阻止了第三方插件运行。", "The log contains quarantine, notarized, or code signature related terms. macOS may be blocking third-party plugins from running."),
            tr!("确认插件来源可信后，按插件说明解除隔离属性或在系统安全设置中允许运行。", "After verifying the plugin is from a trusted source, follow the plugin's instructions to remove quarantine attributes or allow it in System Security settings."),
        ))
    } else {
        None
    }
}

fn check_application_crash(idx: usize, line: &str) -> Option<Finding> {
    if !is_crash_marker_line(line) {
        return None;
    }
    let culprit = line
        .split("because of the plugin: ")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let title = match culprit {
        Some(name) => tr_fmt!(
            "X-Plane 崩溃——插件 {name}",
            "X-Plane crash -- plugin {name}"
        ),
        None => tr!(
            "X-Plane 本次运行发生崩溃",
            "X-Plane crashed during this session"
        )
        .to_string(),
    };
    let explanation = match culprit {
        Some(name) => tr_fmt!("X-Plane 明确记录了崩溃，并指出原因来自插件：{name}。优先检查该插件的版本兼容性和已知问题。", "X-Plane explicitly recorded the crash and identified a plugin as the cause: {name}. Prioritize checking this plugin's version compatibility and known issues."),
        None => tr!("日志明确显示 X-Plane 崩溃。崩溃行本身通常不是根因，需要结合它前面最后加载的插件、飞机、机场或图形错误判断。", "The log clearly shows X-Plane crashed. The crash line itself is usually not the root cause; correlate it with the last loaded plugin, aircraft, airport, or graphics error before the crash.").to_string(),
    };
    let suggestion = match culprit {
        Some(name) => tr_fmt!("先更新 {name} 到最新版；若问题依旧则临时禁用该插件复测。", "Update {name} to the latest version; if the problem persists, temporarily disable this plugin and retest."),
        None => tr!("先看本报告中的高置信错误；如果没有明确错误，用默认飞机、默认机场、禁用第三方插件的方式做干净复测。", "First review high-confidence findings in this report. If no clear errors are found, do a clean retest with default aircraft, default airport, and third-party plugins disabled.").to_string(),
    };
    Some(Finding::new(
        Severity::High,
        Confidence::High,
        "application_crash",
        title,
        evidence_line(idx, line),
        explanation,
        suggestion,
    ))
}

fn check_plugin_encountered_error(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    let channel = parse_xplane_error_channel(line)?;
    if channel != "E/PLG" || !lower.contains("encountered error") {
        return None;
    }
    let plugin_name = line
        .split("Plugin ")
        .nth(1)
        .and_then(|rest| rest.split(" encountered error").next())
        .and_then(|name| name.split(" (").next())
        .map(str::trim)
        .unwrap_or("unknown plugin");
    Some(Finding::new(
        Severity::Medium,
        Confidence::High,
        "plugin_encountered_error",
        tr_fmt!(
            "插件 {plugin_name} 在运行时遇到错误",
            "Plugin {plugin_name} encountered a runtime error"
        ),
        evidence_line(idx, line),
        tr_fmt!(
            "X-Plane 在运行时通过 E/PLG 通道报告了插件 {plugin_name} 的错误。这是 X-Plane 自身识别到的运行时故障，不只是初始化噪声。",
            "X-Plane reported a runtime error for plugin {plugin_name} via the E/PLG channel. This is a runtime fault identified by X-Plane itself, not just initialization noise."
        ),
        tr_fmt!(
            "更新 {plugin_name} 到最新版本，若问题依旧则临时禁用该插件复测。",
            "Update {plugin_name} to the latest version; if the problem persists, temporarily disable the plugin and retest."
        ),
    ))
}

fn extract_nearby_aircraft_path(lines: &[&str], idx: usize) -> Option<String> {
    lines
        .iter()
        .skip(idx)
        .take(8)
        .filter_map(|line| {
            let start = line.find("Aircraft/")?;
            let rest = &line[start..];
            let end = rest
                .find(".acf")
                .map(|pos| pos + ".acf".len())
                .unwrap_or(rest.len());
            Some(rest[..end].trim().to_string())
        })
        .find(|path| !path.is_empty())
}

fn check_aircraft_open_failure(
    idx: usize,
    line: &str,
    lower: &str,
    lines: &[&str],
) -> Option<Finding> {
    if !lower.contains("failed to open the following aircraft")
        && !lower.contains("unknown aircraft")
    {
        return None;
    }
    let aircraft = extract_nearby_aircraft_path(lines, idx)
        .or_else(|| {
            line.find("Aircraft/")
                .map(|start| line[start..].trim().to_string())
        })
        .unwrap_or_else(|| tr!("未知飞机", "unknown aircraft").to_string());

    Some(Finding::new(
        Severity::High,
        Confidence::High,
        "aircraft_open_failure",
        tr_fmt!(
            "X-Plane 无法打开飞机文件：{aircraft}",
            "X-Plane could not open aircraft file: {aircraft}"
        ),
        evidence_line(idx, line),
        tr_fmt!(
            "X-Plane 明确提示飞机文件缺失、损坏，或不是当前版本可读取的飞机文件。目标飞机是：{aircraft}。",
            "X-Plane explicitly reported that the aircraft file is missing, corrupt, or not a readable aircraft file for this version. Target aircraft: {aircraft}."
        ),
        tr!(
            "先不要按插件或显卡方向排查；改用默认飞机复测。若默认飞机正常，重新安装或更新证据中的飞机包，确认 .acf 文件存在且支持当前 X-Plane 版本。",
            "Do not start with plugin or GPU troubleshooting; retest with a default aircraft first. If the default aircraft works, reinstall or update the aircraft package named in the evidence, and verify the .acf file exists and supports this X-Plane version."
        ),
    ))
}

fn check_duplicate_scenery(idx: usize, line: &str, lower: &str) -> Option<Finding> {
    if lower.contains("we found a duplicate runway")
        || lower.contains("duplicate airport")
        || lower.contains("duplicate scenery")
    {
        Some(Finding::new(
            Severity::Medium,
            Confidence::Medium,
            "duplicate_scenery_log",
            tr!("日志提示存在重复机场或风景", "Duplicate airport or scenery detected in log"),
            evidence_line(idx, line),
            tr!("重复机场或风景包可能导致显示异常、跑道重叠或加载顺序问题。", "Duplicate airport or scenery packages can cause display anomalies, overlapping runways, or load order issues."),
            tr!("检查 Custom Scenery 中是否安装了多个覆盖同一机场的包，并查看 scenery_packs.ini 顺序。", "Check if multiple packages covering the same airport are installed in Custom Scenery, and review the scenery_packs.ini ordering."),
        ))
    } else {
        None
    }
}

// ── post-scan findings ──────────────────────────────────────────────

fn scenery_load_crash_context_finding(lines: &[&str]) -> Option<Finding> {
    let crash_idx = lines.iter().position(|line| is_crash_marker_line(line))?;
    let start = crash_idx.saturating_sub(140);
    let window = &lines[start..crash_idx];

    let airport = window
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.contains("I/FLT: Init ") && line.contains(" apt:"))
        .map(|(idx, line)| (start + idx, *line));
    let loading_airport = window
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.contains("I/SCN: Loading sim objects for airport"))
        .map(|(idx, line)| (start + idx, *line));
    let custom_scenery = window
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.contains("Custom Scenery/"))
        .map(|(idx, line)| (start + idx, *line));

    if loading_airport.is_none() && custom_scenery.is_none() {
        return None;
    }

    let primary = loading_airport.or(custom_scenery).or(airport)?;
    let mut finding = Finding::new(
        Severity::Medium,
        Confidence::Medium,
        "scenery_load_crash_context",
        tr!(
            "崩溃发生在机场或地景加载阶段",
            "Crash while loading airport or scenery"
        ),
        evidence_line(primary.0, primary.1),
        tr!(
            "日志在崩溃前出现了机场/地景加载线索。这不是百分百根因，但比很早出现的网络或机场数据库提示更接近崩溃点，适合优先作为复测方向。",
            "The log shows airport/scenery loading context immediately before the crash. This is not a guaranteed root cause, but it is closer to the crash point than earlier network or airport database noise and is a better first retest target."
        ),
        tr!(
            "优先用默认机场复测；如果只在这个机场附近崩溃，临时禁用证据里提到的 Custom Scenery 包，再重启 X-Plane 复现。若复测通过，再逐个恢复地景包定位具体项。",
            "First retest at a default airport. If the crash only happens near this airport, temporarily disable the Custom Scenery package named in the evidence, restart X-Plane, and reproduce. If the retest passes, restore scenery packages one by one to isolate the item."
        ),
    );

    for candidate in [airport, loading_airport, custom_scenery] {
        if let Some((idx, line)) = candidate {
            push_extra_evidence(&mut finding, evidence_line(idx, line));
        }
    }

    Some(finding)
}

fn last_loaded_context_finding(lines: &[&str]) -> Option<Finding> {
    last_interesting_load_line(lines).map(|evidence| {
        Finding::new(
            Severity::Info,
            Confidence::Low,
            "last_loaded_context",
            tr!("崩溃前最后加载线索", "Last loaded context before crash"),
            evidence,
            tr!("这不是直接错误，但如果 X-Plane 启动后马上崩溃，最后加载的插件、飞机或机场常常有排查价值。", "This is not a direct error, but if X-Plane crashed shortly after startup, the last loaded plugin, aircraft, or airport is often worth investigating."),
            tr!("如果高置信错误不足以定位问题，可以围绕这条线索附近的插件、飞机或 scenery 做禁用复测。", "If high-confidence findings are not enough to locate the problem, try disabling plugins, aircraft, or scenery near this clue and retest."),
        )
    })
}

fn pre_crash_context_finding(lines: &[&str]) -> Option<Finding> {
    last_non_noise_line_before_crash(lines).map(|evidence| {
        Finding::new(
            Severity::Info,
            Confidence::Medium,
            "pre_crash_context",
            tr!("崩溃前最后一行日志", "Last log line before crash"),
            evidence,
            tr!("这是崩溃前日志的最后一行有意义内容，直接看这一行附近往往比回溯更远的 'Loaded:' 更快定位问题。", "This is the last meaningful log line before the crash. Looking near this line is often faster at locating the issue than backtracking to distant 'Loaded:' entries."),
            tr!("如果 last_loaded_context 离崩溃很远，优先从这一行前后 30 行的上下文开始排查。", "If last_loaded_context is far from the crash, prioritize investigating the ~30 lines surrounding this line."),
        )
    })
}

fn early_silent_crash_finding(lines: &[&str], findings: &[Finding]) -> Option<Finding> {
    early_silent_crash_hint(lines, findings).map(|evidence| {
        Finding::new(
            Severity::Info,
            Confidence::Low,
            "early_silent_crash",
            tr!("启动早期静默崩溃——建议排查外部因素", "Early silent crash -- external factors likely"),
            evidence,
            tr!("X-Plane 在启动后极短时间内崩溃，且日志中没有高置信错误。这类崩溃的根因可能在 Log.txt 之外：端口冲突（如 8888 被占用）、杀毒软件拦截、文件权限不足、系统服务干扰等。", "X-Plane crashed shortly after startup with no high-confidence errors in the log. The root cause may be outside Log.txt: port conflicts (e.g. port 8888 in use), antivirus blocking, insufficient file permissions, system service interference, etc."),
            tr!("检查 X-Plane Web API 端口 8888 是否被占用（netstat -ano | findstr 8888）；暂时关闭杀毒软件和后台服务后复测；如果最近安装了新软件或系统更新，排查其对端口和文件访问的影响。", "Check if X-Plane Web API port 8888 is in use (netstat -ano | findstr 8888); temporarily disable antivirus and background services and retest. If new software or system updates were recently installed, investigate their impact on ports and file access."),
        )
    })
}

fn abrupt_termination_finding(lines: &[&str]) -> Option<Finding> {
    abrupt_termination_hint(lines).map(|evidence| {
        Finding::new(
            Severity::Info,
            Confidence::Low,
            "abrupt_termination",
            tr!("日志异常截断——进程可能被外部终止", "Log truncated abnormally -- process may have been externally terminated"),
            evidence,
            tr!("X-Plane 的日志在飞行中途突然中断，没有写入崩溃标记也没有正常退出记录。可能原因包括 GPU 驱动超时恢复 (TDR)、系统内存耗尽、热保护关机、外部程序终止进程，或者用户手动结束了 X-Plane。Log.txt 本身无法确认具体原因。", "X-Plane's log truncated mid-flight with no crash marker and no clean shutdown. Possible causes: GPU driver timeout recovery (TDR), system memory exhaustion, thermal shutdown, external program termination, or manual force-quit. Log.txt cannot confirm the specific cause."),
            tr!("如果是意外崩溃，查看 Windows 事件查看器 (eventvwr) 中应用程序和系统日志，寻找与 X-Plane 或显卡驱动相关的错误记录；检查 GPU 温度和驱动版本。如果是有意结束进程则忽略本条。", "If this was an unexpected crash, check Windows Event Viewer (eventvwr) Application and System logs for errors related to X-Plane or GPU drivers; check GPU temperature and driver version. If the process was intentionally terminated, ignore this finding."),
        )
    })
}

// ── main entry point ─────────────────────────────────────────────────

pub fn analyze_log(log_text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let lines: Vec<&str> = log_text.lines().collect();
    findings.extend(detect_xplane_subsystem_errors(&lines));
    let is_macos_log = log_text
        .lines()
        .take(250)
        .any(|line| is_macos_system_line(&line.to_ascii_lowercase()));

    for (idx, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        let vr = extract_vulkan_result_code(line);

        findings.extend(check_plugin_load_failed(idx, line, &lower));
        if let Some(code) = vr {
            findings.push(check_vulkan_result_error(idx, line, Some(code)).unwrap());
        } else {
            findings.extend(check_graphics_api_error(idx, line, &lower));
        }
        findings.extend(check_memory_vram(idx, line, &lower));
        findings.extend(check_texture_vram_pressure(idx, line, &lower));
        findings.extend(check_missing_scenery_asset(idx, line, &lower));
        findings.extend(check_missing_scenery_object(idx, line, &lower));
        findings.extend(check_missing_scenery_library(idx, line, &lower));
        findings.extend(check_missing_global_scenery_tile(idx, line, &lower));
        if vr.is_none() {
            findings.extend(check_vulkan_device_lost(idx, line, &lower));
        }
        findings.extend(check_threading_violation(idx, line, &lower, &lines));
        findings.extend(check_xplm_bypass(idx, line, &lower));
        findings.extend(check_flywithlua_error(idx, line, &lower));
        findings.extend(check_third_party_plugin_error(idx, line, &lower));
        findings.extend(check_plugin_encountered_error(idx, line, &lower));
        findings.extend(check_aircraft_open_failure(idx, line, &lower, &lines));
        if is_macos_log {
            findings.extend(check_macos_security_block(idx, line, &lower));
        }
        findings.extend(check_application_crash(idx, line));
        findings.extend(check_duplicate_scenery(idx, line, &lower));
    }

    if log_indicates_crash(&lines) || log_has_gpu_crash_signal(&lines) {
        findings.extend(scenery_load_crash_context_finding(&lines));
        findings.extend(last_loaded_context_finding(&lines));
        findings.extend(pre_crash_context_finding(&lines));
        findings.extend(early_silent_crash_finding(&lines, &findings));
    } else {
        findings.extend(abrupt_termination_finding(&lines));
    }

    let mut findings = dedupe_findings(findings);
    suppress_generic_findings(&mut findings);
    findings
}

fn is_crash_marker_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("this application has crashed")
        || lower.contains("--=={uuid:")
        || lower.contains("--=={file:")
        || lower.contains("forwarding exception")
        || lower.contains("x-plane cannot continue running")
        || lower.contains("无法继续运行")
        || lower.contains("thread fatal assert")
}

fn log_indicates_crash(lines: &[&str]) -> bool {
    lines.iter().any(|line| is_crash_marker_line(line))
}

fn log_has_gpu_crash_signal(lines: &[&str]) -> bool {
    lines
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("vk_error_device_lost"))
}

fn detect_xplane_subsystem_errors(lines: &[&str]) -> Vec<Finding> {
    let mut by_channel = BTreeMap::<String, Finding>::new();

    for (idx, line) in lines.iter().enumerate() {
        let Some(channel) = parse_xplane_error_channel(line) else {
            continue;
        };

        let finding = by_channel.entry(channel.clone()).or_insert_with(|| Finding {
            severity: xplane_channel_severity(&channel),
            confidence: Confidence::High,
            kind: "xplane_subsystem_errors".to_string(),
            title: format!("X-Plane subsystem {channel} messages"),
            evidence: evidence_line(idx, line),
            occurrences: 0,
            extra_evidence: Vec::new(),
            explanation: format!(
                "X-Plane logged warning/error lines on the {channel} channel. This is a broad structural scan of X-Plane's own log format, so it can catch real issues even when no specific keyword rule matches."
            ),
            suggestion: xplane_channel_suggestion(&channel).to_string(),
        });

        finding.occurrences += 1;
        if finding.occurrences > 1 {
            push_extra_evidence(finding, evidence_line(idx, line));
        }
    }

    by_channel.into_values().collect()
}

fn parse_xplane_error_channel(line: &str) -> Option<String> {
    let mut rest = line.trim_start();
    let first_colon = rest.find(':')?;
    if !rest[..first_colon].chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    rest = &rest[first_colon + 1..];

    let second_colon = rest.find(':')?;
    if !rest[..second_colon].chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    rest = &rest[second_colon + 1..];

    let time_end = rest.find(char::is_whitespace)?;
    let seconds = &rest[..time_end];
    let (whole_seconds, fraction) = seconds.split_once('.')?;
    if whole_seconds.is_empty()
        || fraction.is_empty()
        || !whole_seconds.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }

    rest = rest[time_end..].trim_start();
    let level = rest.chars().next()?;
    if level != 'E' && level != 'W' {
        return None;
    }

    rest = rest.get(1..)?;
    if !rest.starts_with('/') {
        return None;
    }

    let channel_end = rest.find(':')?;
    let subsystem = &rest[1..channel_end];
    if subsystem.is_empty()
        || !subsystem
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.'))
    {
        return None;
    }

    Some(format!("{level}/{subsystem}"))
}

fn xplane_channel_severity(channel: &str) -> Severity {
    if channel.starts_with("E/APT")
        || channel.starts_with("W/APT")
        || channel.starts_with("E/DSF")
        || channel.starts_with("W/DSF")
        || channel.starts_with("E/PLG")
        || channel.starts_with("W/PLG")
        || channel.starts_with("E/NET")
        || channel.starts_with("W/NET")
        || channel.starts_with("E/IDENT")
        || channel.starts_with("W/IDENT")
        || channel.starts_with("E/NVAPI")
        || channel.starts_with("W/NVAPI")
        || channel.starts_with("W/TEX")
    {
        Severity::Info
    } else if channel.starts_with("E/SOUN") || channel.starts_with("W/SOUN") {
        Severity::Low
    } else if channel.starts_with("E/JOY") || channel.starts_with("W/JOY") {
        Severity::Low
    } else if channel.starts_with("E/SYS") || channel.starts_with("W/SYS") {
        Severity::Low
    } else if channel.starts_with("E/SCN") || channel.starts_with("W/SCN") {
        Severity::Medium
    } else if channel.starts_with("W/RLP") {
        Severity::High
    } else if channel.starts_with("E/") {
        Severity::High
    } else {
        Severity::Medium
    }
}

fn xplane_channel_suggestion(channel: &str) -> &'static str {
    if channel.starts_with("E/APT") || channel.starts_with("W/APT") {
        tr!("通常是机场数据库的 background noise，尤其是 Global Airports 发出的。仅当问题出现在某个具体附加机场时才需要排查。", "Usually background airport database noise, especially when it comes from Global Airports. Only investigate if the issue follows a named add-on airport.")
    } else if channel.starts_with("E/PLG") || channel.starts_with("W/PLG") {
        tr!("插件子系统消息通常是初始化噪声（如 'Sim is not yet started'）。仅当消息指向具体的插件故障或崩溃时才需要排查。", "Plugin subsystem messages are usually initialization noise (e.g. 'Sim is not yet started'). Only investigate if the message points to a specific plugin failure or crash.")
    } else if channel.starts_with("E/DSF") || channel.starts_with("W/DSF") {
        tr!("通常是 scenery 数据质量背景信息。仅当问题跟随指定的 tile 或包出现时才需要排查。", "Usually scenery data quality background. Only investigate if the problem follows the named tile or package.")
    } else if channel.starts_with("E/SCN") || channel.starts_with("W/SCN") {
        tr!("检查证据行中提到的 scenery 或对象路径，确认该包已完整安装并处于正确的加载顺序中。", "Check the scenery or object path named in the evidence lines, then verify the package is installed completely and in the expected load order.")
    } else if channel.starts_with("E/JOY") || channel.starts_with("W/JOY") {
        tr!("打开 X-Plane 的摇杆设置，重新校准证据行中提到的设备。", "Open X-Plane's joystick settings and recalibrate the device named in the evidence lines.")
    } else if channel.starts_with("E/SYS") || channel.starts_with("W/SYS") {
        tr!("这是 X-Plane 面向用户的告警。阅读证据行内容，但将其视为检查项，除非明确提到 X-Plane 无法继续运行或当前会话崩溃。", "This is a user-facing X-Plane alert. Read the evidence line, but treat it as a check item unless it explicitly says X-Plane cannot continue or the current session crashed.")
    } else if channel.starts_with("E/NET") || channel.starts_with("W/NET") {
        tr!("通常影响局域网发现、联机广播或网络功能。除非用户的问题正是联网/多人联机，否则不要把它当作崩溃主因。", "Usually affects LAN discovery, multiplayer broadcast, or network features. Unless the user's symptom is networking or multiplayer related, do not treat it as the main crash cause.")
    } else if channel.starts_with("E/IDENT") || channel.starts_with("W/IDENT") {
        tr!("Usually product identity, license receipt, or online identity background. Treat as background unless the user's symptom is activation or sign-in failure.", "Usually product identity, license receipt, or online identity background. Treat as background unless the user's symptom is activation or sign-in failure.")
    } else if channel.starts_with("E/NVAPI") || channel.starts_with("W/NVAPI") {
        tr!("Usually NVIDIA driver capability or permission probing noise during startup. Treat as background unless graphics features are missing or the log later shows a GPU crash.", "Usually NVIDIA driver capability or permission probing noise during startup. Treat as background unless graphics features are missing or the log later shows a GPU crash.")
    } else if channel.starts_with("E/SOUN") || channel.starts_with("W/SOUN") {
        tr!("Usually an aircraft/plugin sound-bank or audio-device check item. Investigate if the user's symptom is missing sound, but do not treat it as the main crash cause by itself.", "Usually an aircraft/plugin sound-bank or audio-device check item. Investigate if the user's symptom is missing sound, but do not treat it as the main crash cause by itself.")
    } else if channel.starts_with("W/TEX") {
        tr!("通常是纹理封装警告。将其视为背景信息，除非相关飞机或 scenery 有明显纹理问题。", "Usually a texture packaging warning. Treat it as background unless the affected aircraft or scenery has visible texture problems.")
    } else if channel.starts_with("W/ART") {
        tr!("某个附加组件修改了 X-Plane 的艺术控制。如果视觉效果、天气或性能异常，禁用相关插件或飞机附加组件做干净复测。", "An add-on modified X-Plane art controls. Disable the named plugin or aircraft add-on for a clean retest if visuals, weather, or performance look wrong.")
    } else if channel.starts_with("W/RLP") {
        tr!("X-Plane 的运行循环被阻塞。将其视为性能/卡顿症状：降低 scenery 和插件负载，然后复测同一路线。仅在用户已观察到问题是在系统或驱动变更后才发生时，才需要对比 GPU 驱动版本或 Windows 电源设置。", "X-Plane's runloop is backed up. Treat this as a performance/stall symptom: reduce scenery and plugin load, then retest the same route. Only compare GPU driver versions or Windows power settings if the user already observed the issue starting after a system or driver change.")
    } else {
        tr!("查看证据行中指定的 X-Plane 子系统，如果问题影响飞行，禁用相关附加组件后复测。", "Review the evidence lines for the named X-Plane subsystem, then retest with related add-ons disabled if the issue affects the flight.")
    }
}

fn is_macos_system_line(lower: &str) -> bool {
    lower.starts_with("os:") && (lower.contains("macos") || lower.contains("mac os"))
        || lower.starts_with("macos")
        || lower.starts_with("darwin")
}

fn last_interesting_load_line(lines: &[&str]) -> Option<String> {
    lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| is_interesting_load_context(line))
        .map(|(idx, line)| evidence_line(idx, line))
}

fn last_non_noise_line_before_crash(lines: &[&str]) -> Option<String> {
    let crash_idx = lines.iter().position(|line| is_crash_marker_line(line))?;

    for i in (0..crash_idx).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("--=={") || line == "upload successful" {
            continue;
        }
        return Some(evidence_line(i, line));
    }

    None
}

fn early_silent_crash_hint(lines: &[&str], findings: &[Finding]) -> Option<String> {
    let has_explanatory_high = findings
        .iter()
        .any(|f| f.severity == Severity::High && f.kind != "application_crash");
    if has_explanatory_high {
        return None;
    }

    let crash_idx = lines.iter().position(|line| is_crash_marker_line(line))?;

    let (last_idx, last_line) = (0..crash_idx).rev().find_map(|i| {
        let line = lines[i].trim();
        if !line.is_empty() && !line.starts_with("--=={") && line != "upload successful" {
            Some((i, line))
        } else {
            None
        }
    })?;

    let sim_secs = parse_sim_seconds(last_line)?;
    if sim_secs < EARLY_CRASH_SIM_SECONDS {
        Some(evidence_line(last_idx, last_line))
    } else {
        None
    }
}

fn parse_sim_seconds(line: &str) -> Option<f64> {
    let line = line.trim();
    let first_colon = line.find(':')?;
    let hours: f64 = line[..first_colon].parse().ok()?;

    let rest = &line[first_colon + 1..];
    let second_colon = rest.find(':')?;
    let minutes: f64 = rest[..second_colon].parse().ok()?;

    let rest = &rest[second_colon + 1..];
    let dot = rest.find('.')?;
    let seconds: f64 = rest[..dot].parse().ok()?;

    let rest = &rest[dot + 1..];
    let millis_end = rest.find(' ').unwrap_or(rest.len());
    let millis: f64 = rest[..millis_end].parse().ok()?;

    Some(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
}

fn abrupt_termination_hint(lines: &[&str]) -> Option<String> {
    let has_clean_shutdown = lines
        .iter()
        .any(|line| line.contains("----- X-Plane has shut down -----"));
    if has_clean_shutdown {
        return None;
    }

    let (last_idx, last_line) = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| !line.trim().is_empty() && parse_sim_seconds(line).is_some())?;

    let sim_secs = parse_sim_seconds(last_line)?;
    if sim_secs < ABRUPT_TERMINATION_MIN_SIM_SECONDS {
        return None;
    }

    Some(evidence_line(last_idx, last_line))
}

fn is_interesting_load_context(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("unload")
        || lower.contains("shutdown")
        || lower.contains("cleanup")
        || lower.contains("complete")
    {
        return false;
    }

    lower.contains("loaded:")
        || lower.contains("loading aircraft")
        || lower.contains("loading scenery")
        || lower.contains("loading plugin")
        || lower.contains("starting plugin")
}

fn dedupe_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut by_kind = BTreeMap::<String, usize>::new();
    let mut out: Vec<Finding> = Vec::new();
    for mut finding in findings {
        let dedupe_key = finding_dedupe_key(&finding);
        if let Some(existing_idx) = by_kind.get(&dedupe_key).copied() {
            let existing = &mut out[existing_idx];
            if finding.severity < existing.severity {
                let previous_evidence = std::mem::replace(&mut existing.evidence, finding.evidence);
                existing.severity = finding.severity;
                existing.confidence = finding.confidence;
                existing.title = finding.title;
                existing.explanation = finding.explanation;
                existing.suggestion = finding.suggestion;
                push_extra_evidence(existing, previous_evidence);
            } else {
                push_extra_evidence(existing, finding.evidence);
            }
            existing.occurrences += finding.occurrences;
            for evidence in finding.extra_evidence.drain(..) {
                push_extra_evidence(existing, evidence);
            }
        } else {
            by_kind.insert(dedupe_key, out.len());
            out.push(finding);
        }
    }
    out
}

fn suppress_generic_findings(findings: &mut Vec<Finding>) {
    let has_specific = |kind: &str| findings.iter().any(|f| f.kind == kind);

    let has_aircraft_failure = has_specific("aircraft_open_failure");
    let has_scenery_missing = has_specific("missing_scenery_object")
        || has_specific("missing_scenery_asset")
        || has_specific("missing_scenery_library");
    let has_gpu_error = has_specific("vulkan_device_lost")
        || has_specific("vulkan_result_error")
        || has_specific("graphics_vulkan_error");
    let has_plugin_issue =
        has_specific("plugin_load_failed") || has_specific("plugin_encountered_error");

    for finding in findings.iter_mut() {
        if finding.kind != "xplane_subsystem_errors" {
            continue;
        }
        let channel = extract_channel_from_subsystem_title(&finding.title);
        let should_suppress = match channel.as_deref() {
            Some("E/SYS") | Some("W/SYS") => {
                has_aircraft_failure && evidence_mentions_aircraft(finding)
            }
            Some("E/ACF") | Some("W/ACF") => {
                has_aircraft_failure && evidence_mentions_aircraft(finding)
            }
            Some("E/SCN") | Some("W/SCN") | Some("E/OBJ") => has_scenery_missing,
            Some(ch) if ch.starts_with("E/GFX") => has_gpu_error,
            Some("E/PLG") | Some("W/PLG") => has_plugin_issue,
            _ => false,
        };
        if should_suppress {
            finding.severity = Severity::Info;
            finding.confidence = Confidence::Low;
        }
    }
}

fn extract_channel_from_subsystem_title(title: &str) -> Option<String> {
    let start = title.find("subsystem ")? + "subsystem ".len();
    let end = title[start..].find(" messages")?;
    Some(title[start..start + end].to_string())
}

fn evidence_mentions_aircraft(finding: &Finding) -> bool {
    let check = |s: &str| {
        let lower = s.to_ascii_lowercase();
        lower.contains("failed to open")
            || lower.contains(".acf")
            || lower.contains("aircraft/")
            || lower.contains("unknown aircraft")
    };
    check(&finding.evidence) || finding.extra_evidence.iter().any(|e| check(e))
}

fn finding_dedupe_key(finding: &Finding) -> String {
    if finding.kind == "xplane_subsystem_errors" || finding.kind == "third_party_plugin_error" {
        format!("{}:{}", finding.kind, finding.title)
    } else {
        finding.kind.clone()
    }
}

fn push_extra_evidence(finding: &mut Finding, evidence: String) {
    if finding.extra_evidence.len() < MAX_EXTRA_EVIDENCE && evidence != finding.evidence {
        finding.extra_evidence.push(evidence);
    }
}

fn evidence_line(idx: usize, line: &str) -> String {
    format!("Log.txt:{}: {}", idx + 1, truncate_evidence(line.trim()))
}

fn truncate_evidence(line: &str) -> String {
    const MAX_CHARS: usize = 320;
    let mut chars = line.chars();
    let truncated: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated} ...[truncated]")
    } else {
        truncated
    }
}

fn is_graphics_api_error_line(lower: &str) -> bool {
    if lower.contains("opengl extensions") {
        return false;
    }

    (lower.contains("vulkan") || lower.contains("metal"))
        && (lower.contains(" error")
            || lower.contains(" failed")
            || lower.contains(" crash")
            || lower.contains("fatal")
            || lower.contains("cannot"))
}

const OFFICIAL_VULKAN_ERROR_CODES: &[&str] = &[
    "VK_ERROR_OUT_OF_HOST_MEMORY",
    "VK_ERROR_OUT_OF_DEVICE_MEMORY",
    "VK_ERROR_INITIALIZATION_FAILED",
    "VK_ERROR_DEVICE_LOST",
    "VK_ERROR_MEMORY_MAP_FAILED",
    "VK_ERROR_LAYER_NOT_PRESENT",
    "VK_ERROR_EXTENSION_NOT_PRESENT",
    "VK_ERROR_FEATURE_NOT_PRESENT",
    "VK_ERROR_INCOMPATIBLE_DRIVER",
    "VK_ERROR_TOO_MANY_OBJECTS",
    "VK_ERROR_FORMAT_NOT_SUPPORTED",
    "VK_ERROR_FRAGMENTED_POOL",
    "VK_ERROR_UNKNOWN",
    "VK_ERROR_OUT_OF_POOL_MEMORY",
    "VK_ERROR_INVALID_EXTERNAL_HANDLE",
    "VK_ERROR_FRAGMENTATION",
    "VK_ERROR_INVALID_OPAQUE_CAPTURE_ADDRESS",
    "VK_ERROR_SURFACE_LOST_KHR",
    "VK_ERROR_NATIVE_WINDOW_IN_USE_KHR",
    "VK_ERROR_OUT_OF_DATE_KHR",
    "VK_ERROR_INCOMPATIBLE_DISPLAY_KHR",
    "VK_ERROR_INVALID_SHADER_NV",
    "VK_ERROR_VALIDATION_FAILED",
    "VK_ERROR_NOT_PERMITTED",
    "VK_ERROR_FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT",
];

fn extract_vulkan_result_code(line: &str) -> Option<&'static str> {
    OFFICIAL_VULKAN_ERROR_CODES
        .iter()
        .copied()
        .find(|code| line.contains(code))
}

fn vulkan_result_severity(code: &str) -> Severity {
    match code {
        "VK_ERROR_DEVICE_LOST"
        | "VK_ERROR_OUT_OF_DEVICE_MEMORY"
        | "VK_ERROR_OUT_OF_HOST_MEMORY"
        | "VK_ERROR_INCOMPATIBLE_DRIVER"
        | "VK_ERROR_INITIALIZATION_FAILED" => Severity::High,
        _ => Severity::Medium,
    }
}

fn vulkan_result_explanation(code: &str) -> &'static str {
    match code {
        "VK_ERROR_DEVICE_LOST" => tr!("Vulkan 报告逻辑或物理设备丢失。这通常是图形设备/驱动层面的严重错误，X-Plane 很可能无法继续运行。", "Vulkan reports the logical or physical device was lost. This is usually a critical GPU/driver-level error; X-Plane likely cannot continue."),
        "VK_ERROR_OUT_OF_DEVICE_MEMORY" => tr!("Vulkan 报告显存分配失败。常见关联因素是纹理质量过高、高分辨率 scenery、显存压力或显卡驱动问题。", "Vulkan reports device memory allocation failure. Common associated factors: texture quality too high, high-resolution scenery, VRAM pressure, or GPU driver issues."),
        "VK_ERROR_OUT_OF_HOST_MEMORY" => tr!("Vulkan 报告主机内存分配失败。可能和系统内存压力、后台程序或资源加载过重有关。", "Vulkan reports host memory allocation failure. May be related to system memory pressure, background programs, or excessive resource loading."),
        "VK_ERROR_INITIALIZATION_FAILED" => tr!("Vulkan 对象初始化失败。可能和驱动、图形功能支持、插件绘制或 X-Plane 图形初始化有关。", "Vulkan object initialization failed. May be related to drivers, graphics feature support, plugin rendering, or X-Plane graphics initialization."),
        "VK_ERROR_INCOMPATIBLE_DRIVER" => tr!("Vulkan 报告驱动不兼容，当前驱动可能不支持所需的 Vulkan 版本或功能。", "Vulkan reports incompatible driver. The current driver may not support the required Vulkan version or features."),
        "VK_ERROR_EXTENSION_NOT_PRESENT" => tr!("Vulkan 报告请求的扩展不存在，可能是驱动或显卡不支持所需功能。", "Vulkan reports the requested extension is not present. The driver or GPU may not support the required feature."),
        "VK_ERROR_FEATURE_NOT_PRESENT" => tr!("Vulkan 报告请求的功能不存在，可能是显卡或驱动不支持。", "Vulkan reports the requested feature is not present. The GPU or driver may not support it."),
        "VK_ERROR_LAYER_NOT_PRESENT" => tr!("Vulkan 报告请求的 layer 不存在或无法加载，可能和 Vulkan runtime、调试层或驱动安装有关。", "Vulkan reports the requested layer is not present or cannot be loaded. May be related to Vulkan runtime, debug layers, or driver installation."),
        "VK_ERROR_OUT_OF_DATE_KHR" => tr!("Vulkan swapchain 已过期，通常和窗口、显示器、分辨率或全屏状态变化有关。", "Vulkan swapchain is out of date, usually related to window, display, resolution, or fullscreen state changes."),
        "VK_ERROR_SURFACE_LOST_KHR" => tr!("Vulkan surface 已不可用，通常和窗口或显示表面被系统改变有关。", "Vulkan surface is no longer available, usually related to the window or display surface being altered by the system."),
        "VK_ERROR_FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT" => tr!("Vulkan 独占全屏模式丢失，可能和切换窗口、显示器状态或系统抢占全屏有关。", "Vulkan exclusive fullscreen mode was lost. May be related to window switching, display state changes, or system takeover of fullscreen."),
        "VK_ERROR_UNKNOWN" => tr!("Vulkan 返回未知错误。官方语义是无法明确归因的实现或输入问题，需要结合上下文继续排查。", "Vulkan returned an unknown error. The official semantics are an implementation or input issue that cannot be clearly attributed; further investigation with context is needed."),
        _ => tr!("日志包含 Vulkan 官方 VkResult 错误码。这个结果比普通关键词更可靠，应结合错误码前后的日志上下文排查。", "The log contains an official Vulkan VkResult error code. This is more reliable than generic keywords; investigate the log context before and after this error code."),
    }
}

fn vulkan_result_suggestion(code: &str) -> &'static str {
    match code {
        "VK_ERROR_DEVICE_LOST" => tr!("先更新或回退显卡驱动，关闭超频和图形增强插件，用默认飞机+默认机场复测；如果只在某机场发生，再检查该 scenery。", "Update or roll back GPU drivers, disable overclocking and graphics-enhancement plugins, retest with default aircraft at a default airport. If it only happens at a specific airport, investigate that scenery."),
        "VK_ERROR_OUT_OF_DEVICE_MEMORY" => tr!("降低纹理质量、抗锯齿和高分辨率 scenery，关闭不必要插件，再复测同一路线。", "Lower texture quality, anti-aliasing, and high-resolution scenery; disable unnecessary plugins and retest the same route."),
        "VK_ERROR_OUT_OF_HOST_MEMORY" => tr!("关闭后台程序，降低 scenery/插件负载，观察系统内存占用后复测。", "Close background programs, reduce scenery/plugin load, monitor system memory usage, and retest."),
        "VK_ERROR_INCOMPATIBLE_DRIVER" => tr!("安装显卡厂商最新版正式驱动；如果刚更新后才出问题，也可以回退到稳定版本。", "Install the latest official driver from the GPU vendor. If the issue started right after a driver update, roll back to the previous stable version."),
        "VK_ERROR_INITIALIZATION_FAILED" => tr!("先用默认飞机和默认机场启动；若仍失败，更新显卡驱动并检查 X-Plane 图形设置。", "Start with default aircraft at a default airport. If it still fails, update GPU drivers and check X-Plane graphics settings."),
        "VK_ERROR_EXTENSION_NOT_PRESENT" | "VK_ERROR_FEATURE_NOT_PRESENT" => tr!("更新显卡驱动；如果硬件较旧，确认它是否满足 X-Plane 12 的图形要求。", "Update GPU drivers. If the hardware is older, check whether it meets X-Plane 12 graphics requirements."),
        "VK_ERROR_LAYER_NOT_PRESENT" => tr!("检查 Vulkan runtime 和显卡驱动安装是否完整，移除不必要的 Vulkan 调试层或覆盖层工具后复测。", "Check that Vulkan runtime and GPU drivers are fully installed. Remove unnecessary Vulkan debug layers or overlay tools and retest."),
        "VK_ERROR_OUT_OF_DATE_KHR" | "VK_ERROR_SURFACE_LOST_KHR" => tr!("切换窗口/全屏模式，重启 X-Plane；如果常发生，更新驱动并关闭覆盖层工具。", "Toggle window/fullscreen mode, restart X-Plane. If this happens frequently, update drivers and disable overlay tools."),
        "VK_ERROR_FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT" => tr!("改用窗口化或无边框模式，关闭可能抢占全屏的覆盖层工具。", "Switch to windowed or borderless mode, and disable overlay tools that may steal fullscreen focus."),
        _ => tr!("把这条错误码前后 30 行日志一起保存，优先用默认飞机、默认机场、禁用第三方插件做对照复测。", "Save ~30 lines of log context before and after this error code. Prioritize a controlled retest with default aircraft, default airport, and third-party plugins disabled."),
    }
}

fn extract_quoted_asset(line: &str) -> Option<String> {
    let start = line.find('\'')?;
    let rest = &line[start + 1..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plugin_load_failures() {
        let findings = analyze_log("dlerror: The specified module could not be found\n");
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "plugin_load_failed"));
    }

    #[test]
    fn detects_vram_or_memory_pressure() {
        let findings = analyze_log("WARNING: VRAM budget exceeded\n");
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "memory_or_vram"));
    }

    #[test]
    fn extracts_library_hint_from_missing_art_asset() {
        let findings = analyze_log(
            "0:00:13.929 E/SCN: The art asset 'PP146-lib/GroundTextures/CarTireTracks.lin' could not be found.\n",
        );
        let finding = findings
            .iter()
            .find(|finding| finding.kind == "missing_scenery_asset")
            .expect("missing scenery asset finding");
        assert!(finding.explanation.contains("PP146-lib"));
    }

    #[test]
    fn detects_vulkan_device_lost() {
        let findings = analyze_log("Encountered Vulkan device loss error!\n");
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "vulkan_device_lost"));
    }

    #[test]
    fn detects_official_vulkan_result_codes() {
        let findings = analyze_log("vkQueueSubmit returned VK_ERROR_DEVICE_LOST\n");
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "vulkan_result_error"
                && finding.title.contains("VK_ERROR_DEVICE_LOST")));
        assert!(!findings
            .iter()
            .any(|finding| finding.kind == "vulkan_device_lost"));
    }

    #[test]
    fn does_not_treat_opengl_extensions_as_graphics_error() {
        let findings = analyze_log(
            "OpenGL Extensions : GL_ARB_debug_output GL_NV_draw_vulkan_image GL_KHR_no_error\n",
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.kind == "graphics_vulkan_error"));
    }

    #[test]
    fn does_not_emit_last_loaded_context_without_crash() {
        let findings = analyze_log(
            "Loaded: Resources/plugins/Example/win_x64/example.xpl\nPlugin unload complete.\n",
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.kind == "last_loaded_context"));
    }

    #[test]
    fn ignores_unload_lines_for_crash_context() {
        let findings = analyze_log(
            "Loaded: Resources/plugins/Example/win_x64/example.xpl\nPlugin unload complete.\n--=={This application has crashed!}==--\n",
        );
        let context = findings
            .iter()
            .find(|finding| finding.kind == "last_loaded_context")
            .expect("crash context finding");
        assert!(context.evidence.contains("Loaded:"));
        assert!(!context.evidence.contains("unload"));
    }

    #[test]
    fn detects_missing_scenery_library() {
        let findings = analyze_log(
            "It requires an additional library scenery package that is not installed, or it is missing some of its files.\n",
        );
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "missing_scenery_library"));
    }

    #[test]
    fn aggregates_xplane_subsystem_error_channels() {
        let findings = analyze_log(
            "0:00:00.000 E/DSF: first road network problem\n\
             0:00:01.000 E/DSF: second road network problem\n\
             0:00:02.000 W/ART: Art control changed\n\
             0:00:03.000 I/SCN: informational scenery line\n",
        );

        let dsf = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/DSF")
            })
            .expect("E/DSF subsystem finding");
        assert_eq!(dsf.occurrences, 2);
        assert_eq!(dsf.severity, Severity::Info);
        assert_eq!(dsf.extra_evidence.len(), 1);

        let art = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("W/ART")
            })
            .expect("W/ART subsystem finding");
        assert_eq!(art.occurrences, 1);
        assert_eq!(art.severity, Severity::Medium);

        assert!(!findings.iter().any(|finding| {
            finding.kind == "xplane_subsystem_errors" && finding.title.contains("I/SCN")
        }));
    }

    #[test]
    fn parses_xplane_subsystem_channels_with_nested_names() {
        let findings = analyze_log("1:23:45.678 E/GFX/VK: renderer failed\n");
        assert!(findings.iter().any(|finding| {
            finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/GFX/VK")
        }));
    }

    #[test]
    fn treats_scenery_subsystem_errors_as_check_items() {
        let findings = analyze_log(
            "0:04:22.433 E/SCN: Failed to find resource 'security', referenced from file 'Custom Scenery/0_OERK_Azersim_x12/Metro station/'.\n",
        );
        let finding = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/SCN")
            })
            .expect("E/SCN subsystem finding");

        assert_eq!(finding.severity, Severity::Medium);
    }

    #[test]
    fn treats_net_beacon_errors_as_background() {
        let findings = analyze_log(
            "0:00:04.736 W/NET: multicast_socket_udp::init: bind() returned 10013\n\
0:00:04.736 E/NET: Failed to init multicast *BEACON* socket, check log lines above for details!\n",
        );

        let e_net = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/NET")
            })
            .expect("E/NET subsystem finding");
        assert_eq!(e_net.severity, Severity::Info);

        let w_net = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("W/NET")
            })
            .expect("W/NET subsystem finding");
        assert_eq!(w_net.severity, Severity::Info);
    }

    #[test]
    fn treats_ident_and_nvapi_as_background() {
        let findings = analyze_log(
            "0:00:00.000 E/IDENT: Refresh Error Code is: 0\n\
0:00:00.000 E/NVAPI: NvAPI access denied while probing startup capabilities\n",
        );

        let ident = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/IDENT")
            })
            .expect("E/IDENT subsystem finding");
        assert_eq!(ident.severity, Severity::Info);

        let nvapi = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/NVAPI")
            })
            .expect("E/NVAPI subsystem finding");
        assert_eq!(nvapi.severity, Severity::Info);
    }

    #[test]
    fn treats_sound_errors_as_audio_check_items() {
        let findings =
            analyze_log("0:00:00.000 E/SOUN: Could not find default sound output device\n");
        let sound = findings
            .iter()
            .find(|finding| {
                finding.kind == "xplane_subsystem_errors" && finding.title.contains("E/SOUN")
            })
            .expect("E/SOUN subsystem finding");
        assert_eq!(sound.severity, Severity::Low);
    }

    #[test]
    fn detects_third_party_plugin_error_formats() {
        let findings = analyze_log(
            "[FoXeTekPlugin] ERROR|11:14:08.967|UDPSocket::read: no pending data\n\
[SharedFlight][SFImageService.cpp:180]: [ERROR] Found non png file\n\
2026-04-09 11:14:35 [SharedFlight][SFUIGraphic.cpp:42]: [ERROR] CAIRO_STATUS_FILE_NOT_FOUND\n\
WeatherBridge 11:14:45 Request timed out\n",
        );

        let foxetek = findings
            .iter()
            .find(|finding| {
                finding.kind == "third_party_plugin_error"
                    && finding.title.contains("FoXeTekPlugin")
            })
            .expect("FoXeTek plugin error finding");
        assert_eq!(foxetek.severity, Severity::Low);

        let shared_flight = findings
            .iter()
            .find(|finding| {
                finding.kind == "third_party_plugin_error" && finding.title.contains("SharedFlight")
            })
            .expect("SharedFlight plugin error finding");
        assert_eq!(shared_flight.occurrences, 2);

        assert!(findings.iter().any(|finding| {
            finding.kind == "third_party_plugin_error" && finding.title.contains("WeatherBridge")
        }));
    }

    #[test]
    fn detects_scenery_load_crash_context() {
        let log = "\
0:00:15.000 I/FLT: Init dat_p0 type:loc_general_area lat:47.788984 lon:13.004313 psi:149.105074 apt:LOWS rwy:15
0:00:17.965 I/SCN: DSF load time: 10234 for file Custom Scenery/X-Plane Landmarks - Salzburg/Earth nav data/+40+010/+47+013.dsf (0 tris, 0 skipped for 0.0 m^2)
0:00:17.965 I/SCN: Loading sim objects for airport LOWS
--=={This application has crashed!}==--\n";
        let findings = analyze_log(log);
        let finding = findings
            .iter()
            .find(|finding| finding.kind == "scenery_load_crash_context")
            .expect("scenery load crash context finding");

        assert_eq!(finding.severity, Severity::Medium);
        assert!(finding
            .evidence
            .contains("I/SCN: Loading sim objects for airport LOWS"));
        assert!(finding
            .extra_evidence
            .iter()
            .any(|line| line.contains("X-Plane Landmarks - Salzburg")));
    }

    #[test]
    fn aggregates_three_same_kind_findings_with_extra_evidence() {
        let findings = analyze_log(
            "Unable to locate object: Lib/A.obj\nUnable to locate object: Lib/B.obj\nUnable to locate object: Lib/C.obj\n",
        );
        let finding = findings
            .iter()
            .find(|finding| finding.kind == "missing_scenery_object")
            .expect("missing scenery object finding");
        assert_eq!(finding.occurrences, 3);
        assert_eq!(finding.extra_evidence.len(), 2);
    }

    #[test]
    fn caps_aggregated_evidence_at_five_total_lines() {
        let mut log = String::new();
        for idx in 0..10 {
            log.push_str(&format!("Unable to locate object: Lib/{idx}.obj\n"));
        }

        let findings = analyze_log(&log);
        let finding = findings
            .iter()
            .find(|finding| finding.kind == "missing_scenery_object")
            .expect("missing scenery object finding");
        assert_eq!(finding.occurrences, 10);
        assert_eq!(1 + finding.extra_evidence.len(), 5);
    }

    #[test]
    fn does_not_treat_flywithlua_quarantine_dir_as_macos_security_on_windows() {
        let findings = analyze_log(
            "OS: Windows 11\nFlyWithLua: Plugin Scripts Quarantine Dir: E:\\SteamLibrary/steamapps/common/X-Plane 12/Resources/plugins/FlyWithLua/Scripts (Quarantine)/\n",
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.kind == "macos_security_block"));
    }

    #[test]
    fn detects_macos_security_lines_only_for_macos_logs() {
        let findings = analyze_log(
            "OS: macOS\nxpl file is quarantined and code signature verification failed\n",
        );
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "macos_security_block"));
    }

    #[test]
    fn does_not_detect_macos_from_random_applications_path() {
        let findings = analyze_log(
            "OS: Windows 11\nLoaded: D:/Applications/X-Plane 12/Resources/plugins/test.xpl\nxpl file is quarantined\n",
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.kind == "macos_security_block"));
    }

    #[test]
    fn truncates_long_evidence_lines() {
        let evidence = evidence_line(0, &format!("failed to load {}", "x".repeat(1000)));
        assert!(evidence.len() < 380);
        assert!(evidence.contains("[truncated]"));
    }

    #[test]
    fn parses_sim_timestamp_correctly() {
        let secs = parse_sim_seconds("0:00:01.134 D/KTX2: some message");
        assert!((secs.unwrap() - 1.134).abs() < 0.001);

        let secs = parse_sim_seconds("1:23:45.678 E/SYS: error");
        assert!((secs.unwrap() - 5025.678).abs() < 0.001);
    }

    #[test]
    fn detects_early_silent_crash_with_no_high_findings() {
        let log = "\
0:00:00.000 I/INIT: X-Plane starting
0:00:00.500 D/KTX2: loading texture
0:00:01.134 D/KTX2: [Resources/bitmaps/world/moon_NML.ktx2] 1024x1024
--=={This application has crashed!}==--\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "early_silent_crash");
        assert!(hint.is_some(), "should detect early silent crash");
    }

    #[test]
    fn skips_early_silent_crash_when_high_finding_exists() {
        let log = "\
0:00:00.000 I/INIT: X-Plane starting
0:00:01.000 VK_ERROR_DEVICE_LOST
0:00:01.134 D/KTX2: texture
--=={This application has crashed!}==--\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "early_silent_crash");
        assert!(
            hint.is_none(),
            "should not flag when High finding explains crash"
        );
    }

    #[test]
    fn detects_abrupt_termination_without_crash_marker() {
        let log = "\
0:00:00.000 I/INIT: start
0:01:00.000 I/SCN: flying
1:30:00.000 I/SCN: mid flight loading tiles
1:30:05.000 I/SCN: more scenery\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "abrupt_termination");
        assert!(
            hint.is_some(),
            "should detect abrupt termination when log ends mid-flight without crash marker"
        );
    }

    #[test]
    fn skips_abrupt_termination_during_early_loading() {
        let log = "\
0:00:00.000 I/INIT: start
0:01:14.000 I/SCN: still loading startup resources\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "abrupt_termination");
        assert!(
            hint.is_none(),
            "should not flag abrupt termination during short startup/loading logs"
        );
    }

    #[test]
    fn skips_abrupt_termination_when_clean_shutdown_present() {
        let log = "\
0:00:00.000 I/INIT: start
1:00:00.000 I/SCN: flying
1:30:00.000 I/SCN: unloading scenery
1:30:05.000 ----- X-Plane has shut down -----\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "abrupt_termination");
        assert!(
            hint.is_none(),
            "should not flag when X-Plane clean shutdown marker present"
        );
    }

    #[test]
    fn skips_abrupt_termination_when_crash_marker_present() {
        let log = "\
0:00:00.000 I/INIT: start
1:00:00.000 I/SCN: flying
--=={This application has crashed!}==--\n";
        let findings = analyze_log(log);
        // When crash marker present, we get application_crash instead
        assert!(findings.iter().any(|f| f.kind == "application_crash"));
        assert!(!findings.iter().any(|f| f.kind == "abrupt_termination"));
    }

    #[test]
    fn pre_crash_context_works_with_uuid_crash_marker() {
        let log = "\
0:00:00.000 I/INIT: start
0:00:01.000 D/KTX2: texture loading
0:00:01.500 I/SCN: scenery loaded
--=={UUID: 12345678-1234-1234-1234-123456789abc}==--\n";
        let findings = analyze_log(log);
        let pre = findings.iter().find(|f| f.kind == "pre_crash_context");
        assert!(pre.is_some(), "UUID crash should produce pre_crash_context");
        assert!(pre.unwrap().evidence.contains("scenery loaded"));
    }

    #[test]
    fn early_silent_crash_works_with_uuid_crash_marker() {
        let log = "\
0:00:00.000 I/INIT: start
0:00:01.134 D/KTX2: texture
--=={UUID: abcd-efgh}==--\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "early_silent_crash");
        assert!(
            hint.is_some(),
            "UUID crash under 5s should trigger early_silent_crash"
        );
    }

    #[test]
    fn skips_abrupt_termination_when_log_too_short() {
        let log = "\
0:00:00.000 I/INIT: start
0:00:05.000 I/SCN: loading\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "abrupt_termination");
        assert!(hint.is_none(), "should not flag logs under 60s sim time");
    }

    #[test]
    fn skips_early_silent_crash_when_crash_is_late() {
        let log = "\
1:30:00.000 I/INIT: mid-flight
1:30:05.000 D/KTX2: texture
--=={This application has crashed!}==--\n";
        let findings = analyze_log(log);
        let hint = findings.iter().find(|f| f.kind == "early_silent_crash");
        assert!(hint.is_none(), "should not flag late crashes");
    }

    #[test]
    fn detects_xplm_bypass() {
        let log = "VisualXP 1.1.8 (simadditions.com) has bypassed XPLM when calling SDK functions. This is a plugin bug.\n";
        let findings = analyze_log(log);
        let f = findings.iter().find(|f| f.kind == "xplm_bypass");
        assert!(f.is_some(), "should detect XPLM bypass");
        assert!(f.unwrap().title.contains("VisualXP 1.1.8"));
    }

    #[test]
    fn xplm_bypass_title_excludes_plugin_id_and_path() {
        let log = "XTLua 2.2.1 id12 (com.x-plane.xtlua.G:\\SteamLibrary/steamapps/common/X-Plane 12/Aircraft/777-300ER-master/plugins/xtlua/.2.0.5) has bypassed XPLM when calling SDK functions. This is a plugin bug.\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "xplm_bypass")
            .expect("should detect XPLM bypass");
        assert!(f.title.contains("XTLua 2.2.1 id12"));
        assert!(
            !f.title.contains("SteamLibrary"),
            "title should not include the plugin id/path"
        );
    }

    #[test]
    fn detects_texture_vram_mild_over_commit() {
        let log = "0:53:01.007 I/TEX: Target scale moved to 2.000000. Texture usage is 3.62 gb out of 3.58 gb available. Memory headroom is 57.82 mb\n";
        let findings = analyze_log(log);
        let f = findings.iter().find(|f| f.kind == "texture_vram_pressure");
        assert!(
            f.is_some(),
            "should flag over-committed VRAM even when mild"
        );
        assert_eq!(f.unwrap().severity, Severity::Medium);
    }

    #[test]
    fn detects_texture_vram_significant_over_commit() {
        let log = "0:10:08.288 I/TEX: Target scale moved to 4.000000. Texture usage is 6.00 gb out of 5.13 gb available. Memory headroom is 15.00 mb\n";
        let findings = analyze_log(log);
        let f = findings.iter().find(|f| f.kind == "texture_vram_pressure");
        assert!(f.is_some(), "should flag significant over-commit as high");
        assert_eq!(f.unwrap().severity, Severity::High);
    }

    #[test]
    fn detects_texture_vram_low_headroom() {
        let log = "0:10:08.288 I/TEX: Target scale moved to 2.000000. Texture usage is 5.14 gb out of 5.13 gb available. Memory headroom is 85.00 mb\n";
        let findings = analyze_log(log);
        let f = findings.iter().find(|f| f.kind == "texture_vram_pressure");
        assert!(f.is_some(), "should flag when headroom < 100mb");
        assert_eq!(f.unwrap().severity, Severity::Medium);
    }

    #[test]
    fn skips_texture_vram_when_no_pressure() {
        let log = "0:04:05.537 I/TEX: Target scale moved to 1.000000. Texture usage is 375.79 mb out of 7.05 gb available. Memory headroom is 6.86 gb\n";
        let findings = analyze_log(log);
        assert!(findings.iter().all(|f| f.kind != "texture_vram_pressure"));
    }

    #[test]
    fn detects_aftermath_file_crash_marker() {
        let log = "\
0:00:00.000 I/INIT: start
0:01:00.000 I/SCN: flying
--=={FILE: C:\\Users\\test\\AppData\\Local\\Temp\\xplane_crash_reports\\aftermath\\gpu_crash.txt}==--\n";
        let findings = analyze_log(log);
        assert!(
            findings.iter().any(|f| f.kind == "application_crash"),
            "should detect --=={{FILE: ...}}==-- as crash marker"
        );
    }

    #[test]
    fn detects_chinese_crash_text() {
        let log = "\
0:00:00.000 I/INIT: start
0:01:00.000 I/SCN: flying
0:01:30.000 E/SYS: X-Plane 无法继续运行\n";
        let findings = analyze_log(log);
        assert!(
            findings.iter().any(|f| f.kind == "application_crash"),
            "should detect Chinese '无法继续运行' as crash marker"
        );
    }

    #[test]
    fn vk_device_lost_without_crash_marker_enters_crash_branch() {
        let log = "\
0:00:00.000 I/INIT: start
0:01:00.000 I/SCN: flying
0:01:30.000 E/GFX: vkQueueSubmit returned VK_ERROR_DEVICE_LOST\n";
        let findings = analyze_log(log);
        assert!(
            !findings.iter().any(|f| f.kind == "abrupt_termination"),
            "VK_ERROR_DEVICE_LOST should prevent abrupt_termination even without crash marker"
        );
    }

    #[test]
    fn detects_plugin_encountered_error() {
        let log = "\
0:00:00.000 I/INIT: start
0:27:02.000 E/PLG: Plugin BetterPushback-v1.6.1 encountered error: Instance data set during draw callback\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "plugin_encountered_error");
        assert!(f.is_some(), "should detect E/PLG encountered error");
        assert!(
            f.unwrap().title.contains("BetterPushback-v1.6.1"),
            "should extract plugin name"
        );
        assert_eq!(f.unwrap().severity, Severity::Medium);
    }

    #[test]
    fn detects_aircraft_open_failure_with_following_acf_path() {
        let log = "\
0:00:07.780 E/SYS: MACIBM_alert: Failed to open the following aircraft:
0:00:07.780 E/SYS: MACIBM_alert: Aircraft/Misc. Aircraft/PAE-A36-REP/PAE-A36/PAE_REP_A36_Analog.acf
0:00:07.780 E/SYS: MACIBM_alert: This could be because the aircraft file is missing or corrupt, or because it is not really an aircraft file at all.
--=={This application has crashed!}==--
";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "aircraft_open_failure")
            .expect("should detect aircraft open failure");
        assert_eq!(f.severity, Severity::High);
        assert!(f.title.contains("PAE_REP_A36_Analog.acf"));
    }

    #[test]
    fn detects_unknown_aircraft_path_on_same_line() {
        let log = "0:00:07.780 E/SYS: MACIBM_alert: Unknown aircraft Aircraft/Foo/Bar.acf (perhaps it was deleted from disk?)\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "aircraft_open_failure")
            .expect("should detect unknown aircraft");
        assert!(f.title.contains("Aircraft/Foo/Bar.acf"));
    }

    #[test]
    fn detects_missing_object_from_package_w_scn() {
        let log = "0:00:00.000 W/SCN: Missing object test_building.obj from package Custom Scenery/Test_Airport/; replacing with blank\n";
        let findings = analyze_log(log);
        let f = findings.iter().find(|f| {
            f.kind == "missing_scenery_object" && f.evidence.contains("test_building.obj")
        });
        assert!(
            f.is_some(),
            "should detect W/SCN Missing object from package"
        );
    }

    #[test]
    fn upgrades_vram_severity_for_extreme_scale() {
        let log = "0:10:08.288 I/TEX: Target scale moved to 0.250000. Texture usage is 5.14 gb out of 5.13 gb available. Memory headroom is 85.00 mb\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "texture_vram_pressure")
            .expect("should find VRAM pressure");
        assert_eq!(
            f.severity,
            Severity::High,
            "scale 0.25 should upgrade Medium to High"
        );
    }

    #[test]
    fn does_not_upgrade_vram_for_mild_scale() {
        let log = "0:53:01.007 I/TEX: Target scale moved to 0.500000. Texture usage is 5.14 gb out of 5.13 gb available. Memory headroom is 85.00 mb\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "texture_vram_pressure")
            .expect("should find VRAM pressure");
        assert_eq!(
            f.severity,
            Severity::Medium,
            "scale 0.5 should keep Medium severity"
        );
    }

    #[test]
    fn aggregated_vram_uses_highest_severity_evidence() {
        let log = "\
0:10:00.000 I/TEX: Target scale moved to 0.500000. Texture usage is 5.14 gb out of 5.13 gb available. Memory headroom is 85.00 mb
0:10:01.000 I/TEX: Target scale moved to 0.250000. Texture usage is 5.14 gb out of 5.13 gb available. Memory headroom is 85.00 mb\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "texture_vram_pressure")
            .expect("should find VRAM pressure");
        assert_eq!(
            f.severity,
            Severity::High,
            "aggregated findings should keep the highest severity"
        );
        assert!(
            f.evidence.contains("0.250000"),
            "primary evidence should point to the high-severity line"
        );
    }

    #[test]
    fn third_party_plugin_error_excludes_flywithlua() {
        let findings = analyze_log(
            "[FlyWithLua] ERROR: bad argument #1 to 'draw_string' (number expected, got nil)\n",
        );
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "third_party_plugin_error"),
            "FlyWithLua errors should not trigger third_party_plugin_error"
        );
        assert!(
            findings.iter().any(|f| f.kind == "flywithlua_error"),
            "FlyWithLua errors should still be caught by flywithlua_error"
        );
    }

    #[test]
    fn third_party_plugin_error_excludes_xplane_channel() {
        let findings = analyze_log("0:00:00.000 E/PLG: [TestPlugin] ERROR: something failed\n");
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "third_party_plugin_error"),
            "X-Plane channel lines should not trigger third_party_plugin_error"
        );
    }

    #[test]
    fn third_party_plugin_error_requires_uppercase_error_marker() {
        let findings = analyze_log("[AircraftPlugin]: Parsing error in navaids.txt\n");
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "third_party_plugin_error"),
            "ordinary lowercase error text should not be treated as a plugin ERROR marker"
        );
    }

    #[test]
    fn plugin_encountered_error_title_excludes_object_path() {
        let log = "0:27:02.000 E/PLG: Plugin BetterPushback-v1.6.1 (H:\\X-Plane 12\\Resources\\plugins\\BetterPushback\\objects\\night_lamp.obj) encountered error: Instance data set during draw callback\n";
        let findings = analyze_log(log);
        let f = findings
            .iter()
            .find(|f| f.kind == "plugin_encountered_error")
            .expect("plugin encountered error finding");
        assert!(f.title.contains("BetterPushback-v1.6.1"));
        assert!(
            !f.title.contains("night_lamp.obj"),
            "title should contain the plugin name, not the object path"
        );
    }

    #[test]
    fn skips_abrupt_termination_for_short_logs_below_threshold() {
        let log = "\
0:00:00.000 I/INIT: start
0:01:20.000 I/SCN: still loading scenery\n";
        let findings = analyze_log(log);
        assert!(
            !findings.iter().any(|f| f.kind == "abrupt_termination"),
            "80s sim time should be below the 120s minimum threshold"
        );
    }

    // ── suppression tests ──

    #[test]
    fn suppresses_esys_when_aircraft_open_failure_exists() {
        let log = "\
0:00:00.000 E/SYS: MACIBM_alert: Failed to open the following aircraft:
0:00:00.000 E/SYS: MACIBM_alert: Aircraft/Test/A320.acf
0:00:00.000 E/SYS: MACIBM_alert: This could be because the aircraft file is missing or corrupt.\n";
        let findings = analyze_log(log);
        let esys = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("E/SYS"));
        assert!(esys.is_some(), "E/SYS should still exist");
        assert_eq!(
            esys.unwrap().severity,
            Severity::Info,
            "E/SYS should be downgraded to Info when aircraft_open_failure covers it"
        );
        assert!(
            findings.iter().any(|f| f.kind == "aircraft_open_failure"),
            "aircraft_open_failure should be the primary finding"
        );
    }

    #[test]
    fn suppresses_acf_when_aircraft_open_failure_exists() {
        let log = "\
0:00:00.000 W/ACF: Scanning of aircraft files is complete. Some of your aircraft files will not be available to fly in X-Plane, probably because they were created by a version of X-Plane that is too old. Aircraft that will be ignored:
0:00:00.000 W/ACF:     Aircraft/Test/A320.acf
0:00:01.000 E/SYS: MACIBM_alert: Failed to open the following aircraft:
0:00:01.000 E/SYS: MACIBM_alert: Aircraft/Test/A320.acf\n";
        let findings = analyze_log(log);
        let acf = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("W/ACF"))
            .expect("W/ACF subsystem finding");
        assert_eq!(
            acf.severity,
            Severity::Info,
            "W/ACF should be downgraded when aircraft_open_failure covers it"
        );
    }

    #[test]
    fn preserves_non_aircraft_esys_when_aircraft_failure_exists() {
        let log = "\
0:00:00.000 E/SYS: MACIBM_alert: Failed to open the following aircraft:
0:00:00.000 E/SYS: MACIBM_alert: Aircraft/Test/C172.acf
0:00:30.000 E/SYS: MACIBM_alert: Some other system alert not about aircraft\n";
        let findings = analyze_log(log);
        let esys_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("E/SYS"))
            .collect();
        // E/SYS should still exist (suppressed to Info, not deleted)
        assert!(!esys_findings.is_empty());
        // After suppression, severity should be Info
        for f in &esys_findings {
            assert_eq!(f.severity, Severity::Info);
        }
    }

    #[test]
    fn suppresses_scenery_subsystem_when_missing_object_exists() {
        let log = "\
0:00:00.000 W/SCN: Missing object test.obj from package Custom Scenery/Test/; replacing with blank
0:00:00.000 E/SCN: Failed to find resource 'test.png' referenced from file 'Custom Scenery/Test/'.\n";
        let findings = analyze_log(log);
        let wscn = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("W/SCN"));
        assert!(wscn.is_some());
        assert_eq!(wscn.unwrap().severity, Severity::Info);
        let escn = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("E/SCN"));
        assert!(escn.is_some());
        assert_eq!(escn.unwrap().severity, Severity::Info);
        assert!(
            findings.iter().any(|f| f.kind == "missing_scenery_object"),
            "missing_scenery_object should be the primary finding"
        );
    }

    #[test]
    fn suppresses_gfx_when_vulkan_device_lost_exists() {
        let log = "\
0:00:00.000 E/GFX: Encountered Vulkan device loss error!
0:00:00.000 E/GFX/VK: Encountered Vulkan error VK_ERROR_DEVICE_LOST.\n";
        let findings = analyze_log(log);
        let egfx = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("E/GFX"));
        assert!(egfx.is_some());
        assert_eq!(egfx.unwrap().severity, Severity::Info);
        assert!(
            findings.iter().any(|f| f.kind == "vulkan_device_lost"),
            "vulkan_device_lost should be the primary finding"
        );
    }

    #[test]
    fn suppresses_eplg_when_plugin_encountered_error_exists() {
        let log = "\
0:00:00.000 E/PLG: Plugin TestPlugin encountered error: something went wrong\n";
        let findings = analyze_log(log);
        let eplg = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("E/PLG"));
        assert!(eplg.is_some());
        assert_eq!(eplg.unwrap().severity, Severity::Info);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "plugin_encountered_error"),
            "plugin_encountered_error should be the primary finding"
        );
    }

    #[test]
    fn preserves_generic_finding_when_no_specific_rule_exists() {
        let log = "\
0:00:00.000 E/SYS: MACIBM_alert: Some system alert
0:00:00.000 E/GFX: Some graphics message
0:30:00.000 I/SCN: flying\n";
        let findings = analyze_log(log);
        let esys = findings
            .iter()
            .find(|f| f.kind == "xplane_subsystem_errors" && f.title.contains("E/SYS"));
        assert!(esys.is_some());
        assert_eq!(
            esys.unwrap().severity,
            Severity::Low,
            "E/SYS should keep Low severity when no aircraft_open_failure exists"
        );
    }
}
