fn main() {
    let attributes = tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_config",
            "save_config",
            "toggle_config",
            "quit_app",
            "pick_folder",
        ]),
    );
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
