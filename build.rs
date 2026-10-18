//! Linka a libcups. Com `libcups2-dev` instalado usamos `-lcups` normal;
//! sem ele (só o runtime `libcups.so.2`), linkamos direto no soname.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let has_dev_symlink = ["/usr/lib/x86_64-linux-gnu/libcups.so", "/usr/lib/libcups.so", "/usr/lib64/libcups.so"]
        .iter()
        .any(|p| Path::new(p).exists());

    if has_dev_symlink {
        println!("cargo:rustc-link-lib=dylib=cups");
    } else {
        println!("cargo:warning=libcups2-dev não encontrado; linkando em libcups.so.2 diretamente");
        println!("cargo:rustc-link-arg=-l:libcups.so.2");
    }
}
