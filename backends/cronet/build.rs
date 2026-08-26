use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=android/src/main/java");
    println!("cargo:rerun-if-changed=android/api");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("android_bindings.rs");
    let sources = vec![
        manifest_dir.join("android/src/main/java"),
        manifest_dir.join("android/api"),
    ];
    let patterns = [
        "io.nyquest.cronet.NativeUrlRequestCallback",
        "org.chromium.net.CronetEngine",
        "org.chromium.net.UrlRequest",
        "org.chromium.net.UploadDataProvider",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    jbindgen::Builder::new()
        .root_path("crate::bindings")
        .input_sources(sources, Vec::new(), patterns)
        .generate_native_interfaces(true)
        .generate()
        .expect("failed to generate Cronet JNI bindings")
        .write_to_file(output)
        .expect("failed to write Cronet JNI bindings");
}
