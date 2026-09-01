fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    if std::env::var_os("FFMPEG_DIR").is_some() {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN/../lib/skwd-paper");
    }
}
