fn main() {
    for variable in [
        "OPENAIDE_DESKTOP_UPDATE_ORIGIN",
        "OPENAIDE_DESKTOP_UPDATE_RELEASE_PUBLIC_KEY",
        "OPENAIDE_DESKTOP_UPDATE_RECOVERY_PUBLIC_KEY",
        "OPENAIDE_DESKTOP_UPDATE_ENABLED_BUILD",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    tauri_build::build()
}
