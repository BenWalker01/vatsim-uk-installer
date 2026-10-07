fn main() {
    println!("cargo:rerun-if-changed=data/logo.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("data/logo.ico")
            .compile()
            .expect("failed to embed the application icon");
    }
}
