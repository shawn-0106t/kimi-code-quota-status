// Embeds a Windows VERSIONINFO resource into the exe (winresource, SPEC section 11
// build-dependency whitelist). FileVersion/ProductVersion are sourced from
// CARGO_PKG_VERSION, so tag = Cargo.toml = exe file properties stay in lockstep
// with the release workflow's tag<->version consistency check. The resource is
// ~1 KB and lives in .rsrc, which `strip = true` (symbols only) leaves untouched.

fn main() {
    // Only the Windows target carries a version resource to embed
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut res = winresource::WindowsResource::new();
    res.set("FileDescription", "Kimi Code CLI statusline quota display");
    res.set("ProductName", "quota-status");
    res.set("OriginalFilename", "quota-status.exe");
    res.set(
        "LegalCopyright",
        "Copyright (c) 2026 Shawn Qi (shawn-0106t)",
    );
    res.set("FileVersion", &version);
    res.set("ProductVersion", &version);
    res.compile()
        .expect("failed to embed Windows version resource");
    println!("cargo:rerun-if-changed=build.rs");
}
