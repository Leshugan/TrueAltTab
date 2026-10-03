use std::process::Command;
fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    let obj = format!("{}/res.o", out);
    let st = Command::new("x86_64-w64-mingw32-windres")
        .args(["res/app.rc", "-O", "coff", "-o", &obj])
        .status()
        .expect("windres");
    assert!(st.success());
    println!("cargo:rustc-link-arg={}", obj);
    println!("cargo:rerun-if-changed=res/app.rc");
    println!("cargo:rerun-if-changed=res/app.manifest");
    println!("cargo:rerun-if-changed=res/TrueAltTab.ico");
}
