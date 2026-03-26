//! Build script for Rusty-404
//!
//! Links platform-specific frameworks for inter-app video (Syphon, NDI).

fn main() {
    #[cfg(target_os = "macos")]
    {
        // ===== Syphon Framework =====
        //
        // Search order:
        //   1. SYPHON_FRAMEWORK_DIR env var (user override)
        //   2. <workspace>/../syphon-rs/syphon-lib/  (local dev checkout)
        //   3. $CARGO_HOME/git/checkouts/syphon-rs-*/*/syphon-lib/  (cargo git dep cache)
        let syphon_framework_dir = find_syphon_framework()
            .expect(
                "Syphon.framework not found. Either:\n  \
                 - Set SYPHON_FRAMEWORK_DIR to the directory containing Syphon.framework\n  \
                 - Clone https://github.com/BlueJayLouche/syphon-rs next to this repo\n  \
                 - Run `cargo fetch` to populate the git dep cache",
            );

        let syphon_dir = syphon_framework_dir.to_string_lossy().into_owned();

        // Framework search path
        println!("cargo:rustc-link-arg=-F{}", syphon_dir);

        // Link the framework
        println!("cargo:rustc-link-arg=-framework");
        println!("cargo:rustc-link-arg=Syphon");

        // Rpath so the framework is found at runtime
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", syphon_dir);
        println!("cargo:warning=Syphon framework found at: {}", syphon_dir);

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
        println!("cargo:rerun-if-env-changed=SYPHON_FRAMEWORK_DIR");
        println!("cargo:rerun-if-env-changed=CARGO_HOME");
    }
}

#[cfg(target_os = "macos")]
fn find_syphon_framework() -> Option<std::path::PathBuf> {
    // 1. User override
    if let Ok(dir) = std::env::var("SYPHON_FRAMEWORK_DIR") {
        let p = std::path::PathBuf::from(dir);
        if p.join("Syphon.framework").exists() {
            return Some(p);
        }
    }

    // 2. Local dev checkout: <workspace>/../syphon-rs/syphon-lib/
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(parent) = manifest.parent() {
        let candidate = parent.join("syphon-rs/syphon-lib");
        if candidate.join("Syphon.framework").exists() {
            return Some(candidate);
        }
    }

    // 3. Cargo git dep cache: $CARGO_HOME/git/checkouts/syphon-rs-*/*/syphon-lib/
    let cargo_home = std::env::var("CARGO_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join(".cargo")));

    if let Some(cargo_home) = cargo_home {
        let checkouts = cargo_home.join("git/checkouts");
        if let Ok(entries) = std::fs::read_dir(&checkouts) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                // Match any checkout directory that starts with "syphon-rs"
                if name.starts_with("syphon-rs") {
                    // Each entry has sub-directories per revision
                    if let Ok(revs) = std::fs::read_dir(entry.path()) {
                        for rev in revs.flatten() {
                            let candidate = rev.path().join("syphon-lib");
                            if candidate.join("Syphon.framework").exists() {
                                return Some(candidate);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}
