//! Windows : intègre l'icône et les informations de version dans novaterm.exe.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/novaterm.ico");
    #[cfg(windows)]
    {
        let ico = "assets/icon/novaterm.ico";
        if std::path::Path::new(ico).exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(ico);
            res.set("FileDescription", "NovaTerm — terminal moderne");
            res.set("ProductName", "NovaTerm");
            if let Err(e) = res.compile() {
                println!("cargo:warning=icône Windows non intégrée : {e}");
            }
        }
    }
}
