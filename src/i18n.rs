use std::sync::RwLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    Zh,
    En,
}

static LOCALE: RwLock<Locale> = RwLock::new(Locale::En);

pub fn set_locale(locale: Locale) {
    *LOCALE.write().unwrap() = locale;
}

pub fn get_locale() -> Locale {
    *LOCALE.read().unwrap()
}

pub fn is_zh() -> bool {
    matches!(get_locale(), Locale::Zh)
}

#[macro_export]
macro_rules! tr {
    ($zh:literal, $en:literal) => {
        match $crate::i18n::get_locale() {
            $crate::i18n::Locale::Zh => $zh,
            $crate::i18n::Locale::En => $en,
        }
    };
}

#[macro_export]
macro_rules! tr_fmt {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::get_locale() {
            $crate::i18n::Locale::Zh => format!($zh $(, $arg)*),
            $crate::i18n::Locale::En => format!($en $(, $arg)*),
        }
    };
}

pub fn detect_locale() -> Locale {
    // 1. XPLANE_DOCTOR_LANG env var
    if let Ok(val) = std::env::var("XPLANE_DOCTOR_LANG") {
        match val.to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-tw" | "zh_hans" | "zh_hant" => return Locale::Zh,
            "en" | "en-us" | "en-gb" => return Locale::En,
            _ => {}
        }
    }

    // 2. Unix LANG/LC_ALL
    for var in ["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            if val.to_ascii_lowercase().contains("zh") {
                return Locale::Zh;
            }
        }
    }

    // 3. Windows: check system UI language via registry
    #[cfg(windows)]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Control Panel\International",
                "/v",
                "LocaleName",
            ])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.to_ascii_lowercase().contains("zh") {
                return Locale::Zh;
            }
        }
    }

    Locale::En
}
