fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        // Use absolute path to ensure it's found
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let icon_path = std::path::Path::new(&manifest_dir).join("assets/gifs/ameath.ico");
        res.set_icon(icon_path.to_str().unwrap());
        res.compile().unwrap();
    }
}
