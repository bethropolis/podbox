fn main() {
    println!(
        "cargo:rustc-env=PODMGR_VERSION={}",
        std::env::var("CARGO_PKG_VERSION").unwrap()
    );
}
