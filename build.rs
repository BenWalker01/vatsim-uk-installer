fn main() {
    println!("cargo:rerun-if-changed=data/logo.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("data/logo.ico")
            .set("ProductName", "VATSIM UK Installer")
            .set("FileDescription", "VATSIM UK Controller Pack Installer")
            .set("CompanyName", "Ben Walker")
            .set("LegalCopyright", "Open source (see LICENCE)")
            .set("OriginalFilename", "vatsim-uk-installer.exe")
            .compile()
            .expect("failed to embed the application icon");
    }
}
