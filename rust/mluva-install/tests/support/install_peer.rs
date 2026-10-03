//! Unshipped dependency-load endpoint for transaction faults. Real GTK loading
//! is verified separately through the complete installed production bundle.
fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("MLUVA_INSTALL_PEER").unwrap());
    std::fs::write(root.join("dependency-check"), b"loaded\n").unwrap();
    if std::env::var("MLUVA_INSTALL_CASE").unwrap() == "dependency-failure" {
        std::process::exit(73);
    }
}
