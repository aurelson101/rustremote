fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // VTE 0.84 on Darwin allocates its descriptor scan buffer on the main stack.
        println!("cargo:rustc-link-arg-bin=rustremote=-Wl,-stack_size,0x2000000");
    }
    println!("cargo:rerun-if-changed=assets/icons.gresource.xml");
    println!("cargo:rerun-if-changed=assets/icons");
    let target = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"))
        .join("rustremote.gresource");
    let status = std::process::Command::new("glib-compile-resources")
        .args([
            "assets/icons.gresource.xml",
            "--sourcedir=assets",
            "--target",
        ])
        .arg(target)
        .status()
        .expect("glib-compile-resources est requis pour les icônes");
    assert!(
        status.success(),
        "Compilation des ressources graphiques impossible"
    );
}
