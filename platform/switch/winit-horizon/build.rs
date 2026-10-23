fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for name in [
        "android_platform",
        "web_platform",
        "macos_platform",
        "ios_platform",
        "windows_platform",
        "apple",
        "free_unix",
        "redox",
        "x11_platform",
        "wayland_platform",
        "orbital_platform",
        "unreleased_changelogs",
    ] {
        println!("cargo:rustc-check-cfg=cfg({name})");
    }
}
