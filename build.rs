//! Встраивает иконку и версию в exe. Требует компилятор ресурсов
//! (rc.exe на MSVC, windres на GNU); если его нет — dev-сборка просто
//! продолжается без иконки, релизы собирает CI на MSVC.

fn main() {
    println!("cargo:rerun-if-changed=assets/bufernet.ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/bufernet.ico");
    if let Err(e) = res.compile() {
        println!("cargo:warning=icon resource skipped (no resource compiler): {e}");
    }
}
