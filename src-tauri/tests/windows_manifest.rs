#![cfg(target_os = "windows")]

use std::ffi::c_void;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn FindResourceW(module: *mut c_void, name: *const u16, kind: *const u16) -> *mut c_void;
    fn LoadResource(module: *mut c_void, resource: *mut c_void) -> *mut c_void;
    fn LockResource(resource: *mut c_void) -> *mut c_void;
    fn SizeofResource(module: *mut c_void, resource: *mut c_void) -> u32;
}

#[test]
fn test_binary_embeds_common_controls_v6_manifest() {
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        assert!(!module.is_null());
        let resource = FindResourceW(module, 1_usize as *const u16, 24_usize as *const u16);
        assert!(
            !resource.is_null(),
            "test binary is missing its application manifest"
        );
        let loaded = LoadResource(module, resource);
        assert!(!loaded.is_null());
        let bytes = LockResource(loaded);
        assert!(!bytes.is_null());
        let length = SizeofResource(module, resource) as usize;
        let manifest = std::str::from_utf8(std::slice::from_raw_parts(bytes.cast::<u8>(), length))
            .expect("manifest should be UTF-8");
        assert!(manifest.contains("Microsoft.Windows.Common-Controls"));
        assert!(manifest.contains("version=\"6.0.0.0\""));
    }
}

#[cfg(target_env = "gnu")]
#[test]
fn gnu_build_copies_webview_loader_beside_application() {
    let application = std::path::Path::new(env!("CARGO_BIN_EXE_cc-switch"));
    let loader = application.parent().unwrap().join("WebView2Loader.dll");
    let bytes = std::fs::read(&loader)
        .unwrap_or_else(|error| panic!("missing WebView2 loader at {}: {error}", loader.display()));
    assert!(
        bytes.starts_with(b"MZ"),
        "WebView2 loader is not a PE image"
    );
}
