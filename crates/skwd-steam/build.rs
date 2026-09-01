use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("linux") {
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(target_dir) = out_dir.ancestors().nth(3).map(PathBuf::from) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(target_dir.join("build")) else {
        return;
    };
    let source = entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("steamworks-sys-"))
        .map(|entry| entry.path().join("out").join("libsteam_api.so"))
        .filter(|path| path.is_file())
        .filter_map(|path| Some((path.metadata().ok()?.modified().ok()?, path)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path);
    let Some(source) = source else {
        return;
    };
    let dest = target_dir.join("libsteam_api.so");
    if let Err(error) = std::fs::copy(&source, &dest) {
        println!("cargo:warning=failed to copy libsteam_api.so: {error}");
    } else {
        println!("cargo:rerun-if-changed={}", source.display());
    }
}
