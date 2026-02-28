// Emits cfg flags based on profile and features.
//
// - `is_production`: profile is production (dstack hardcodes socket, main skips warning)
// - `phala`: phala code is active. Mandatory in production (profile); optional in dev (--features phala)
fn main() {
    println!("cargo:rustc-check-cfg=cfg(is_production)");
    println!("cargo:rustc-check-cfg=cfg(phala)");

    let profile = std::env::var("PROFILE").unwrap_or_default();
    let phala_feature = std::env::var("CARGO_FEATURE_PHALA").is_ok();

    if profile == "production" {
        println!("cargo:rustc-cfg=is_production");
    }
    // phala: mandatory in production, or explicitly enabled for dev/testing
    if profile == "production" || phala_feature {
        println!("cargo:rustc-cfg=phala");
    }
}
