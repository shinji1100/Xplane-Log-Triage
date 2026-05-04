use std::path::Path;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target != "x86_64-pc-windows-gnu" {
        return;
    }

    for path in ["C:/msys64/mingw64/lib", "C:/msys64/ucrt64/lib"] {
        if Path::new(path).join("libshlwapi.a").exists() {
            println!("cargo:rustc-link-arg-bin=xplane-doctor-gui=-L{path}");
            break;
        }
    }
}
