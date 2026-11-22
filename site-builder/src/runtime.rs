//! Content-named modules prevent a new page from running yesterday's WASM.
use sha2::{Digest, Sha256};
use std::{fs, io, path::Path};
const FILES: &[&str] = &[
    "engine.js",
    "engine_bg.wasm",
    "phone.js",
    "phone_bg.wasm",
    "worker.js",
    "audio-worklet.js",
    "synthphone.bin",
    "phone.css",
];
fn url(file: &str, key: &str) -> String {
    let (stem, extension) = file.rsplit_once('.').unwrap();
    format!("{stem}.{key}.{extension}")
}
fn imports(source: &str, file: &str, key: &str) -> String {
    source
        .lines()
        .map(|line| {
            // imports["./engine.js"] is the WASM ABI: change ES import URLs only.
            if line.starts_with("import ") {
                line.replace(file, &url(file, key))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
pub fn assets(dist: &Path) -> io::Result<Vec<String>> {
    let version = fs::read_to_string(dist.join("runtime-version.txt"))?;
    let key = version.trim();
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::other("Invalid runtime version"));
    }
    FILES
        .iter()
        .map(|file| {
            let name = url(file, key);
            if !dist.join(&name).is_file() {
                return Err(io::Error::other(format!("Missing runtime asset: {name}")));
            }
            Ok(name)
        })
        .collect()
}
pub fn version(dist: &Path) -> io::Result<()> {
    let mut digest = Sha256::new();
    for file in FILES {
        let bytes = fs::read(dist.join(file))?;
        digest.update(file.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    let key = format!("{:x}", digest.finalize());
    // Remove only this builder's regenerable, content-named output. Keep one
    // copy of each runtime file instead of accumulating bundles on every build.
    for entry in fs::read_dir(dist)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let owned = FILES.iter().any(|file| {
            let (stem, extension) = file.rsplit_once('.').unwrap();
            name.strip_prefix(&format!("{stem}."))
                .and_then(|s| s.strip_suffix(&format!(".{extension}")))
                .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        });
        if owned && entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
        }
    }
    for (file, wasm) in [
        ("engine.js", "engine_bg.wasm"),
        ("phone.js", "phone_bg.wasm"),
    ] {
        let source = fs::read_to_string(dist.join(file))?;
        let source = source.replace(&format!("'{wasm}'"), &format!("'{}'", url(wasm, &key)));
        fs::write(dist.join(file), imports(&source, "./engine.js", &key))?;
    }
    let worker = fs::read_to_string(dist.join("worker.js"))?;
    let worker = imports(&worker, "./phone.js", &key).replace(
        "'./synthphone.bin'",
        &format!("'{}'", url("./synthphone.bin", &key)),
    );
    fs::write(dist.join("worker.js"), worker)?;
    for file in ["index.html", "lab.html"] {
        let mut html = fs::read_to_string(dist.join(file))?;
        for asset in [
            "phone.js",
            "engine.js",
            "worker.js",
            "audio-worklet.js",
            "phone.css",
            "synthphone.bin",
        ] {
            html = html.replace(asset, &url(asset, &key));
        }
        fs::write(dist.join(file), html)?;
    }
    let mut manifest = format!("{{\"version\":\"{key}\",\"files\":{{");
    for (i, file) in FILES.iter().enumerate() {
        if i != 0 {
            manifest.push(',');
        }
        manifest.push_str(&format!("\"{file}\":\"{}\"", url(file, &key)));
        fs::rename(dist.join(file), dist.join(url(file, &key)))?;
    }
    manifest.push_str("}}\n");
    fs::write(dist.join("runtime.json"), manifest)?;
    fs::write(dist.join("runtime-version.txt"), format!("{key}\n"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_wasm_change_invalidates_the_entire_runtime_but_preserves_import_abi() {
        let directory = std::env::temp_dir().join(format!(
            "synthphone-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let generate = |bytes: &[u8]| {
            for file in FILES {
                fs::write(directory.join(file), b"fixture").unwrap();
            }
            fs::write(directory.join("phone_bg.wasm"), bytes).unwrap();
            fs::write(directory.join("phone.js"),"import * as engine from './engine.js';\nimports[\"./engine.js\"] = engine;\nnew URL('phone_bg.wasm',import.meta.url);\n").unwrap();
            fs::write(
                directory.join("engine.js"),
                "new URL('engine_bg.wasm',import.meta.url);\n",
            )
            .unwrap();
            fs::write(directory.join("worker.js"),"import init from './phone.js';\nstart_worker(new URL('./synthphone.bin',import.meta.url).href);\n").unwrap();
            fs::write(directory.join("index.html"),"<script data-worker=\"worker.js\" data-worklet=\"audio-worklet.js\">import init from './phone.js';</script>").unwrap();
            fs::write(
                directory.join("lab.html"),
                "import init from './engine.js';",
            )
            .unwrap();
            version(&directory).unwrap();
            fs::read_to_string(directory.join("runtime-version.txt"))
                .unwrap()
                .trim()
                .to_owned()
        };
        let first = generate(b"old wasm");
        let second = generate(b"new wasm");
        fs::write(directory.join("private.txt"), "must not be published").unwrap();
        let published = assets(&directory).unwrap();
        assert_eq!(published.len(), FILES.len());
        assert!(!published.iter().any(|name| name.contains("private")));
        assert_ne!(first, second);
        for file in FILES {
            assert!(directory.join(url(file, &second)).is_file());
            assert!(!directory.join(url(file, &first)).exists());
        }
        for file in ["index.html", "lab.html"] {
            let source = fs::read_to_string(directory.join(file)).unwrap();
            assert!(source.contains(&second));
            assert!(!source.contains(&first));
        }
        let glue = fs::read_to_string(directory.join(url("phone.js", &second))).unwrap();
        assert!(glue.contains("imports[\"./engine.js\"] = engine;"));
        assert!(glue.contains(&url("phone_bg.wasm", &second)));
        let worker = fs::read_to_string(directory.join(url("worker.js", &second))).unwrap();
        assert!(worker.contains(&url("synthphone.bin", &second)));
        fs::write(directory.join("runtime-version.txt"), "../../private").unwrap();
        assert!(assets(&directory).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
