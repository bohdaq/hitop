fn main() {
    // capabilities/updater/ grants the updater and process plugins, which only the
    // `updater` feature (GitHub Release builds) compiles in. Without it (the Mac App
    // Store build) those permissions don't exist, so only the top-level files count.
    let pattern = if std::env::var_os("CARGO_FEATURE_UPDATER").is_some() {
        "./capabilities/**/*.json"
    } else {
        "./capabilities/*.json"
    };
    println!("cargo:rerun-if-changed=capabilities");
    println!("cargo:rerun-if-changed=capabilities/updater");
    if let Err(error) = tauri_build::try_build(tauri_build::Attributes::new().capabilities_path_pattern(pattern)) {
        println!("{error:#}");
        std::process::exit(1);
    }
}
