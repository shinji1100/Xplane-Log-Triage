use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn choose_detected_install() -> Option<PathBuf> {
    let mut candidates = detect_xplane_installs();
    candidates.sort();
    candidates.into_iter().next()
}

pub fn detect_xplane_installs() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for path in common_xplane_paths() {
        push_if_xplane_root(&mut candidates, path);
    }

    for steam_library in steam_libraries() {
        push_if_xplane_root(
            &mut candidates,
            steam_library
                .join("steamapps")
                .join("common")
                .join("X-Plane 12"),
        );
    }

    dedupe_paths(candidates)
}

fn common_xplane_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from(r"C:\X-Plane 12"),
        PathBuf::from(r"C:\Program Files\X-Plane 12"),
        PathBuf::from(r"C:\Program Files (x86)\X-Plane 12"),
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\X-Plane 12"),
        PathBuf::from(r"C:\Program Files\Steam\steamapps\common\X-Plane 12"),
    ];

    if let Some(user_profile) = env::var_os("USERPROFILE") {
        let user_profile = PathBuf::from(user_profile);
        paths.push(user_profile.join("Desktop").join("X-Plane 12"));
        paths.push(user_profile.join("Documents").join("X-Plane 12"));
        paths.push(user_profile.join("Downloads").join("X-Plane 12"));
    }

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join("X-Plane 12"));
        paths.push(home.join("Applications").join("X-Plane 12"));
        paths.push(
            home.join(".steam")
                .join("steam")
                .join("steamapps")
                .join("common")
                .join("X-Plane 12"),
        );
        paths.push(
            home.join(".local")
                .join("share")
                .join("Steam")
                .join("steamapps")
                .join("common")
                .join("X-Plane 12"),
        );
        paths.push(
            home.join(".var")
                .join("app")
                .join("com.valvesoftware.Steam")
                .join(".local")
                .join("share")
                .join("Steam")
                .join("steamapps")
                .join("common")
                .join("X-Plane 12"),
        );
    }

    paths.push(PathBuf::from("/Applications/X-Plane 12"));
    paths.push(PathBuf::from("/Applications/X-Plane 12/X-Plane.app"));
    paths.push(PathBuf::from("/opt/X-Plane 12"));
    paths
}

fn steam_libraries() -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    let mut steam_roots = vec![
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
    ];

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        steam_roots.push(home.join(".steam").join("steam"));
        steam_roots.push(home.join(".local").join("share").join("Steam"));
        steam_roots.push(
            home.join(".var")
                .join("app")
                .join("com.valvesoftware.Steam")
                .join(".local")
                .join("share")
                .join("Steam"),
        );
    }

    for steam_root in steam_roots {
        if steam_root.exists() {
            libraries.push(steam_root.clone());
            let vdf_path = steam_root.join("steamapps").join("libraryfolders.vdf");
            if let Ok(text) = fs::read_to_string(vdf_path) {
                libraries.extend(parse_steam_libraryfolders(&text));
            }
        }
    }

    libraries
}

pub(crate) fn parse_steam_libraryfolders(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("\"path\"") {
            continue;
        }
        let quoted: Vec<&str> = trimmed.split('"').collect();
        if quoted.len() >= 4 {
            paths.push(PathBuf::from(quoted[3].replace("\\\\", "\\")));
        }
    }
    paths
}

fn push_if_xplane_root(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if is_xplane_root(&path) {
        candidates.push(path);
    }
}

fn is_xplane_root(path: &Path) -> bool {
    path.join("Log.txt").exists()
        || path.join("X-Plane.exe").exists()
        || path.join("X-Plane").exists()
        || path.join("X-Plane.app").is_dir()
        || path.join("Contents").join("MacOS").join("X-Plane").exists()
        || path.join("Resources").join("plugins").exists()
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for path in paths {
        let key = path.to_string_lossy().to_ascii_lowercase();
        if seen.insert(key) {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steam_library_paths() {
        let text = r#"
        "libraryfolders"
        {
            "0"
            {
                "path"      "C:\\Program Files (x86)\\Steam"
            }
            "1"
            {
                "path"      "D:\\SteamLibrary"
            }
        }
        "#;
        let paths = parse_steam_libraryfolders(text);
        assert!(paths
            .iter()
            .any(|path| path == &PathBuf::from(r"D:\SteamLibrary")));
    }
}
