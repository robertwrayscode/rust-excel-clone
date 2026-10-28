//! Embeds the app icon and version info into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/rexcel.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/rexcel.ico");
        res.set("ProductName", "RExcel");
        res.set("FileDescription", "RExcel spreadsheet");
        if let Err(e) = res.compile() {
            // Don't fail the build if the resource compiler is missing; the exe just has no icon.
            println!("cargo:warning=could not embed Windows icon: {}", e);
        }
    }
}
