//! Build script for Rusty-404
//!
//! Links platform-specific frameworks for inter-app video (Syphon, NDI).

fn main() {
    #[cfg(target_os = "macos")]
    {
        // ===== Syphon Framework =====
        let syphon_framework_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| {
                let candidate = p.join("syphon-rs/syphon-lib");
                if candidate.join("Syphon.framework").exists() {
                    Some(candidate)
                } else {
                    None
                }
            })
            .or_else(|| {
                std::env::var("SYPHON_FRAMEWORK_DIR")
                    .ok()
                    .map(std::path::PathBuf::from)
            })
            .expect(
                "Syphon.framework not found. Set SYPHON_FRAMEWORK_DIR to the directory \
                 containing Syphon.framework, or place it at <workspace>/../syphon-rs/syphon-lib/",
            );
        let syphon_framework_dir = syphon_framework_dir.to_string_lossy().into_owned();

        // Framework search path
        println!("cargo:rustc-link-arg=-F{}", syphon_framework_dir);

        // Link the framework
        println!("cargo:rustc-link-arg=-framework");
        println!("cargo:rustc-link-arg=Syphon");

        // Rpath so the framework is found at runtime
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", syphon_framework_dir);

        // Link required system frameworks
        println!("cargo:rustc-link-lib=framework=IOSurface");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AVFoundation");

        // ===== NDI Library =====
        // libndi.dylib install name is @rpath/libndi.dylib, so we need to add
        // the NDI SDK lib directory as an rpath for the binary to find it.
        let ndi_lib_paths = [
            "/usr/local/lib",
            "/Library/NDI SDK for Apple/lib/macOS",
        ];

        for path in &ndi_lib_paths {
            if std::path::Path::new(path).exists() {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path);
            }
        }

        // ===== Bundle-friendly rpaths =====
        // For .app bundles: look in Contents/Frameworks next to the executable
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");

        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=../syphon-rs/syphon-lib/Syphon.framework");
    }
}
