//! Write the app icon to a real `.ico` file (EXP29).
//!
//! `build.rs` already encodes the EXP14 icon into `OUT_DIR`, but a build
//! artifact is not a path the release pipeline can hand to the installer's
//! `SetupIconFile`. This example reuses exactly the same encoder - the source is
//! included verbatim, the same trick `build.rs` uses - so the setup icon, the
//! embedded exe icon and the runtime `WM_SETICON` icon cannot drift apart.
//!
//! ```powershell
//! cargo run -p airqlality --example emit_icon -- target\AirQlality.ico
//! ```

#[path = "../src/icon_design.rs"]
mod icon_design;
#[allow(dead_code)] // only the icon-relevant constants are read here
#[path = "../src/theme.rs"]
mod theme;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: emit_icon <out.ico>");
        std::process::exit(2);
    };
    let ico = icon_design::encode_ico(&icon_design::ICO_SIZES);
    if let Some(dir) = std::path::Path::new(&path).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).expect("create the icon's directory");
        }
    }
    std::fs::write(&path, &ico).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {path} ({} bytes)", ico.len());
}
