//! Request the production build from the repository's default branch.
use std::{path::Path, process::Command};

pub fn publish(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    super::run(Command::new("gh").current_dir(root).args([
        "workflow",
        "run",
        "publish.yml",
        "--repo",
        "UnitedCode/virtual-synthphone-e",
        "--ref",
        "main",
    ]))?;
    println!("Requested the GitHub Pages rebuild from current app and firmware main.");
    Ok(())
}
