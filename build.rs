fn main() {
    if let Ok("release") = std::env::var("PROFILE").as_deref() {
        // TODO: make the include directory configurable (also at build time)
        println!("cargo::rerun-if-changed=frontend");
    }
}
