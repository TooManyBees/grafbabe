fn main() {
    if std::env::var_os("CARGO_FEATURE_INCLUDE_HTML").is_some() {
        println!("cargo::rerun-if-changed=frontend");
    }
}
