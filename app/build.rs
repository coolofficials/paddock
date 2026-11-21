use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=frontend/dist");
    println!("cargo:rerun-if-changed=assets/webview2/WebView2Loader.dll");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let directory = PathBuf::from(env::var("OUT_DIR").unwrap());
    // GNU builds dynamically link the redistributable WebView2 loader.
    // Place it beside the executable as well as including it in release packages.
    let profile = directory.ancestors().nth(3).expect("Cargo profile directory");
    fs::copy("assets/webview2/WebView2Loader.dll",profile.join("WebView2Loader.dll")).expect("WebView2 loader copy failed");
    let output = directory.join("app.o");
    let resource = directory.join("app.rc");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let parts = ["CARGO_PKG_VERSION_MAJOR", "CARGO_PKG_VERSION_MINOR", "CARGO_PKG_VERSION_PATCH"].map(|name| env::var(name).unwrap().parse::<u16>().unwrap().to_string()).join(",");
    let template = fs::read_to_string("assets/app.rc").unwrap();
    fs::write(&resource, template.replace("APP_VERSION_PARTS", &format!("{parts},0")).replace("APP_VERSION_TEXT", &version)).unwrap();
    let status = Command::new("windres.exe").arg(&resource).args(["-I", "assets", "-O", "coff", "-o"]).arg(&output).status().expect("Project-local windres was not found. Use scripts/build.ps1.");
    assert!(status.success(), "Resource compilation failed");
    println!("cargo:rustc-link-arg={}", output.display());
}
