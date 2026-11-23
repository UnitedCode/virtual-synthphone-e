//! Build the same Cortex-M7 application that is flashed on the physical phone.
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Error = Box<dyn std::error::Error>;
pub const TARGET: &str = "thumbv7em-none-eabihf";
const FIXTURE_REVISION: &str = "2084ca93a36fb60b6c4fec4947f2376c4ef08dac";

pub struct Image {
    pub path: PathBuf,
    pub hotspots: PathBuf,
    pub revision: String,
    pub sha256: String,
}

fn output(command: &mut Command) -> Result<String, Error> {
    let result = command.output()?;
    if !result.status.success() {
        return Err(format!("{command:?}: {}", String::from_utf8_lossy(&result.stderr)).into());
    }
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

fn clean_revision(source: &Path) -> Result<String, Error> {
    if !output(
        Command::new("git")
            .current_dir(source)
            .args(["status", "--porcelain"]),
    )?
    .is_empty()
    {
        return Err(
            "Build the firmware from a clean checkout so its Git revision identifies the image"
                .into(),
        );
    }
    output(
        Command::new("git")
            .current_dir(source)
            .args(["rev-parse", "HEAD"]),
    )
}

impl Image {
    pub fn fixture(root: &Path) -> Result<Self, Error> {
        let path = root.join("firmware/synthphone.bin");
        let sha256 = format!("{:x}", Sha256::digest(fs::read(&path)?));
        Ok(Self {
            path,
            sha256,
            revision: FIXTURE_REVISION.into(),
            hotspots: root.join("engine/hotspots.txt"),
        })
    }

    pub fn build(root: &Path, source: &Path, target: &Path) -> Result<Self, Error> {
        let source = source.canonicalize()?;
        let revision = clean_revision(&source)?;
        let version = output(Command::new("rustc").current_dir(&source).arg("-vV"))?;
        let host = version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or("Missing Rust host target")?;
        let sysroot = output(
            Command::new("rustc")
                .current_dir(&source)
                .args(["--print", "sysroot"]),
        )?;
        let objcopy = Path::new(&sysroot)
            .join("lib/rustlib")
            .join(host)
            .join("bin")
            .join(format!("llvm-objcopy{}", std::env::consts::EXE_SUFFIX));
        if !objcopy.is_file() {
            return Err(
                "Install the Rust llvm-tools-preview component for the firmware toolchain".into(),
            );
        }
        super::run(
            Command::new("cargo")
                .current_dir(&source)
                .args([
                    "build",
                    "--locked",
                    "--release",
                    "--target",
                    TARGET,
                    "--bin",
                    "app",
                ])
                .env("CARGO_TARGET_DIR", target)
                .env("CARGO_INCREMENTAL", "0"),
        )?;
        if clean_revision(&source)? != revision {
            return Err("Firmware checkout changed during the build".into());
        }
        let stage = root.join("firmware-build");
        fs::create_dir_all(&stage)?;
        let path = stage.join("synthphone.bin");
        super::run(
            Command::new(objcopy)
                .args(["-O", "binary"])
                .arg(target.join(TARGET).join("release/app"))
                .arg(&path),
        )?;
        let bytes = fs::read(&path)?;
        if !(8..=128 * 1024).contains(&bytes.len()) {
            return Err(format!(
                "ARM image size {} does not fit the phone's flash",
                bytes.len()
            )
            .into());
        }
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let mut hotspots = root.join("engine/hotspots.txt");
        let expected = format!("# SHA-256: {sha256};");
        if !fs::read_to_string(&hotspots)?
            .lines()
            .any(|line| line.starts_with(&expected))
        {
            hotspots = stage.join("hotspots.txt");
            // Boot and profile these exact bytes. Old instruction addresses
            // belong to a different image and cannot tune this build.
            super::run(
                Command::new("cargo")
                    .current_dir(root.join("engine"))
                    .args([
                        "run",
                        "--locked",
                        "--release",
                        "--target",
                        host,
                        "--features",
                        "profile",
                        "--example",
                        "realtime",
                        "--",
                    ])
                    .arg(&hotspots)
                    .arg("all")
                    .env("CARGO_TARGET_DIR", target)
                    .env("CARGO_INCREMENTAL", "0")
                    .env("CARGO_PROFILE_RELEASE_LTO", "false")
                    .env("VD_INTERPRETER_PROFILE", "1")
                    .env("SYNTHPHONE_IMAGE", &path),
            )?;
        }
        let image = Self {
            path,
            hotspots,
            revision,
            sha256,
        };
        // Run the unchanged audio and MIDI regression against the selected image
        // and its newly generated (or matching) instruction profile.
        super::run(
            Command::new("cargo")
                .current_dir(root.join("engine"))
                .args([
                    "test",
                    "--locked",
                    "--release",
                    "--target",
                    host,
                    "--test",
                    "vocal_dsp",
                    "--test",
                    "live_stream",
                ])
                .env("CARGO_TARGET_DIR", target)
                .env("CARGO_INCREMENTAL", "0")
                .env("CARGO_PROFILE_RELEASE_LTO", "false")
                .env("SYNTHPHONE_IMAGE", &image.path)
                .env("SYNTHPHONE_HOTSPOTS", &image.hotspots),
        )?;
        println!("Firmware {} · SHA-256 {}", image.revision, image.sha256);
        Ok(image)
    }

    pub fn copy_to(&self, dist: &Path) -> Result<(), Error> {
        let bytes = fs::read(&self.path)?;
        if format!("{:x}", Sha256::digest(&bytes)) != self.sha256 {
            return Err("Firmware image changed during the build".into());
        }
        fs::write(dist.join("synthphone.bin"), bytes)?;
        fs::write(
            dist.join("firmware-revision.txt"),
            format!("{}\n", self.revision),
        )?;
        fs::write(
            dist.join("firmware-sha256.txt"),
            format!("{}\n", self.sha256),
        )?;
        Ok(())
    }
}
