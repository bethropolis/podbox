fn main() {
    println!(
        "cargo:rustc-env=PODBOX_VERSION={}",
        std::env::var("CARGO_PKG_VERSION").unwrap()
    );
}
