//! Build a self-contained static site. No server-side runtime is needed.
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
mod firmware;
mod pages;
mod runtime;
mod serve;
fn run(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Build command failed with {status}").into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../engine")
        .canonicalize()?;
    let root = engine.parent().unwrap();
    let dist = root.join("dist");
    let mode = env::args().nth(1);
    if mode.as_deref() == Some("serve") {
        serve::serve(&dist)?;
        return Ok(());
    }
    if mode.as_deref() == Some("pages") {
        return pages::publish(root);
    }
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| engine.join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        env::current_dir()?.join(target)
    };
    let image = match mode.as_deref() {
        Some("build-firmware") => {
            let source = env::args_os()
                .nth(2)
                .ok_or("Usage: build-firmware <firmware checkout>")?;
            firmware::Image::build(root, Path::new(&source), &target)?
        }
        Some("build") | None => firmware::Image::fixture(root)?,
        _ => return Err("Use build, build-firmware, serve or pages".into()),
    };
    run(Command::new("cargo")
        .current_dir(&engine)
        .args([
            "build",
            "--locked",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .env("SYNTHPHONE_IMAGE", &image.path)
        .env("SYNTHPHONE_HOTSPOTS", &image.hotspots)
        .env("CARGO_INCREMENTAL", "0"))?;
    fs::create_dir_all(&dist)?;
    run(Command::new("wasm-bindgen")
        .arg(target.join("wasm32-unknown-unknown/release/virtual_synthphone_e_engine.wasm"))
        .args(["--target", "web", "--out-name", "engine", "--out-dir"])
        .arg(&dist))?;
    fs::copy(engine.join("index.html"), dist.join("lab.html"))?;
    let phone = engine.parent().unwrap().join("phone");
    run(Command::new("cargo")
        .current_dir(&phone)
        .args([
            "build",
            "--locked",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_INCREMENTAL", "0"))?;
    run(Command::new("wasm-bindgen")
        .arg(target.join("wasm32-unknown-unknown/release/synthphone_web_ui.wasm"))
        .args(["--target", "web", "--out-name", "phone", "--out-dir"])
        .arg(&dist))?;
    for file in ["index.html", "phone.css", "worker.js", "audio-worklet.js"] {
        fs::copy(phone.join(file), dist.join(file))?;
    }
    fs::create_dir_all(dist.join("assets"))?;
    for file in [
        "keypad-restored.webp",
        "mobile-encoder.webp",
        "mobile-handset.webp",
        "mobile-mic-knob.webp",
        "phone-base.webp",
        "receiver.webp",
        "phone-on-hook.webp",
        "wood.webp",
    ] {
        fs::copy(
            phone.join("assets").join(file),
            dist.join("assets").join(file),
        )?;
    }
    image.copy_to(&dist)?;
    runtime::version(&dist)?;
    runtime::assets(&dist)?;
    fs::write(dist.join(".nojekyll"), [])?;
    let revision = Command::new("git")
        .current_dir(dist.parent().unwrap())
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !revision.status.success() {
        return Err("Cannot identify the source revision".into());
    }
    fs::write(dist.join("source-revision.txt"), revision.stdout)?;
    println!("Static browser site built at {}", dist.display());
    Ok(())
}
