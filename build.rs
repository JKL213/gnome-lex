//! Kompiliert Blueprint-Dateien, GResources und das GSettings-Schema, damit
//! `cargo build` auch ohne Meson ein lauffähiges Programm liefert.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let data_dir = manifest_dir.join("data");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data/gesetze.gresource.xml");
    println!("cargo:rerun-if-changed=data/style.css");
    println!("cargo:rerun-if-changed=data/org.gnomelex.Gesetze.gschema.xml");
    println!("cargo:rerun-if-changed=data/icons");
    println!("cargo:rerun-if-changed=data/ui");
    for var in ["APP_ID", "VERSION", "PROFILE", "LOCALEDIR", "PKGDATADIR", "GSCHEMA_DIR"] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    compile_blueprints(&data_dir.join("ui"), &out_dir.join("ui"));
    compile_resources(&data_dir, &out_dir);
    compile_schema(&data_dir, &out_dir);
    export_config(&out_dir);
}

fn compile_blueprints(ui_dir: &Path, out_ui_dir: &Path) {
    fs::create_dir_all(out_ui_dir).expect("create ui out dir");
    let mut inputs: Vec<PathBuf> = fs::read_dir(ui_dir)
        .expect("read data/ui")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "blp"))
        .collect();
    inputs.sort();
    for p in &inputs {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    let compiler = env::var("BLUEPRINT_COMPILER").unwrap_or_else(|_| "blueprint-compiler".into());
    let status = Command::new(&compiler)
        .arg("batch-compile")
        .arg(out_ui_dir)
        .arg(ui_dir)
        .args(&inputs)
        .status()
        .unwrap_or_else(|e| panic!("blueprint-compiler ({compiler}) konnte nicht gestartet werden: {e}"));
    assert!(status.success(), "blueprint-compiler ist fehlgeschlagen");
}

fn compile_resources(data_dir: &Path, out_dir: &Path) {
    // Quellverzeichnisse: data/ (CSS, Icons) und OUT_DIR (kompilierte .ui-Dateien).
    let data = data_dir.to_string_lossy().into_owned();
    let out = out_dir.to_string_lossy().into_owned();
    glib_build_tools::compile_resources(
        &[data.as_str(), out.as_str()],
        data_dir
            .join("gesetze.gresource.xml")
            .to_str()
            .expect("gresource path"),
        "gesetze.gresource",
    );
}

fn compile_schema(data_dir: &Path, out_dir: &Path) {
    let schema_dir = out_dir.join("schemas");
    fs::create_dir_all(&schema_dir).expect("create schema dir");
    let src = data_dir.join("org.gnomelex.Gesetze.gschema.xml");
    fs::copy(&src, schema_dir.join("org.gnomelex.Gesetze.gschema.xml")).expect("copy schema");
    let status = Command::new("glib-compile-schemas")
        .arg("--strict")
        .arg(&schema_dir)
        .status()
        .expect("glib-compile-schemas konnte nicht gestartet werden");
    assert!(status.success(), "glib-compile-schemas ist fehlgeschlagen");
    if env::var("GSCHEMA_DIR").is_err() {
        println!("cargo:rustc-env=GSCHEMA_DIR={}", schema_dir.display());
    }
}

/// Meson setzt diese Variablen; ohne Meson gelten Entwicklungs-Standardwerte.
fn export_config(out_dir: &Path) {
    let set = |name: &str, default: String| {
        let value = env::var(name).unwrap_or(default);
        println!("cargo:rustc-env={name}={value}");
    };
    set("APP_ID", "org.gnomelex.Gesetze.Devel".into());
    set("VERSION", env::var("CARGO_PKG_VERSION").unwrap_or_default());
    set("PROFILE", "development".into());
    set("LOCALEDIR", out_dir.join("locale").display().to_string());
    set("PKGDATADIR", out_dir.display().to_string());
}
