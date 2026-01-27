use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("workspace root");
    let dist_dir = root_dir
        .join("frontend")
        .join("packages")
        .join("inspector")
        .join("dist");

    println!("cargo:rerun-if-env-changed=SANDBOX_AGENT_SKIP_INSPECTOR");
    println!("cargo:rerun-if-changed={}", dist_dir.display());

    let skip = env::var("SANDBOX_AGENT_SKIP_INSPECTOR").is_ok();
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let out_file = out_dir.join("inspector_assets.rs");

    if skip {
        write_disabled(&out_file);
        return;
    }

    if !dist_dir.exists() {
        panic!(
            "Inspector frontend missing at {}. Run `pnpm --filter @sandbox-agent/inspector build` (or `pnpm -C frontend/packages/inspector build`) or set SANDBOX_AGENT_SKIP_INSPECTOR=1 to skip embedding.",
            dist_dir.display()
        );
    }

    let dist_literal = quote_path(&dist_dir);
    let contents = format!(
        "pub const INSPECTOR_ENABLED: bool = true;\n\
         pub fn inspector_dir() -> Option<&'static include_dir::Dir<'static>> {{\n\
             Some(&INSPECTOR_DIR)\n\
         }}\n\
         static INSPECTOR_DIR: include_dir::Dir<'static> = include_dir::include_dir!(\"{}\");\n",
        dist_literal
    );

    fs::write(&out_file, contents).expect("write inspector_assets.rs");
}

fn write_disabled(out_file: &Path) {
    let contents = "pub const INSPECTOR_ENABLED: bool = false;\n\
        pub fn inspector_dir() -> Option<&'static include_dir::Dir<'static>> {\n\
            None\n\
        }\n";
    fs::write(out_file, contents).expect("write inspector_assets.rs");
}

fn quote_path(path: &Path) -> String {
    path.to_str()
        .expect("valid path")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
