//! Build script for Rusty-404
//!
//! Links platform-specific frameworks for inter-app video

fn main() {
    #[cfg(target_os = "macos")]
    {
        // Path to the local Syphon framework
        let framework_path = std::path::PathBuf::from("../crates/syphon/syphon-lib");
        let framework_full = framework_path.canonicalize().unwrap_or_else(|_| framework_path.clone());
        
        if framework_path.join("Syphon.framework").exists() {
            // Add framework search path
            println!("cargo:rustc-link-search=framework={}", framework_full.display());
            // Add rpath so the binary can find the framework at runtime
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", framework_full.display());
        }
        
        // Link required frameworks
        println!("cargo:rustc-link-lib=framework=Syphon");
        println!("cargo:rustc-link-lib=framework=IOSurface");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=Foundation");
        
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=../crates/syphon/syphon-lib/Syphon.framework");
    }
}
