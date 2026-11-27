fn main() {
    let windows_target = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let mut attributes = tauri_build::Attributes::new();
    if windows_target {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attributes).expect("failed to build Tauri resources");

    if windows_target {
        println!("cargo:rerun-if-changed=common-controls.manifest");
        println!("cargo:rerun-if-changed=common-controls.rc");
        embed_resource::compile_for_everything("common-controls.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed Common Controls v6 manifest");
    }
}
