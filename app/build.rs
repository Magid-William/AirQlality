//! Embed the app icon into the executable (EXP14).
//!
//! The pixels come from `src/icon_design.rs`, included verbatim here so the
//! embedded `.ico` and the runtime `WM_SETICON` icon are the same design.
//! `windres` (from the MinGW toolchain AGENTS.md already requires on `PATH`)
//! compiles a one-line `.rc` into a COFF object that we add to the link.
//!
//! This is deliberately hand-rolled rather than pulling in `embed-resource` /
//! `winresource`: the project keeps new crates out, and the whole thing is a
//! handful of lines.

// `icon_design.rs` refers to `crate::theme`, so bring theme in as a module too.
// Both are pure (consts + `std`), so they compile fine inside the build script.
#[path = "src/icon_design.rs"]
mod icon_design;
#[allow(dead_code)] // only the icon-relevant constants are read here
#[path = "src/theme.rs"]
mod theme;

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon_design.rs");
    println!("cargo:rerun-if-changed=src/theme.rs");

    // The icon resource is Windows-only. On any other target the build is a
    // no-op so the crate still compiles.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));

    // 1. Write the icon the app also draws at runtime.
    let ico = out.join("airqlality.ico");
    std::fs::write(&ico, icon_design::encode_ico(&icon_design::ICO_SIZES))
        .expect("write generated .ico");

    // 2. A one-line resource script naming it (resource id 1 = the first icon).
    let rc = out.join("airqlality.rc");
    std::fs::write(&rc, "1 ICON \"airqlality.ico\"\n").expect("write .rc");

    // 3. Compile it to a COFF object and link it into the binary.
    let windres = find_windres();
    let obj = out.join("airqlality_icon.o");
    let status = Command::new(&windres)
        .current_dir(&out)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&obj)
        .arg(&rc)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {windres:?}: {e}"));
    assert!(status.success(), "{windres:?} failed with {status}");
    println!("cargo:rustc-link-arg-bins={}", obj.display());
}

/// Find `windres`: the MinGW toolchain is already required on `PATH`
/// (AGENTS.md), with a fallback to the known local install.
fn find_windres() -> PathBuf {
    if Command::new("windres").arg("--version").output().is_ok() {
        return PathBuf::from("windres");
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let candidate = PathBuf::from(local)
            .join("mingw64")
            .join("bin")
            .join("windres.exe");
        if candidate.exists() {
            return candidate;
        }
    }
    panic!("windres not found: put the MinGW toolchain on PATH (see AGENTS.md)");
}
