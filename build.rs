const GRAFBABE_FRONTEND: &'static str = "GRAFBABE_FRONTEND";

fn main() {
    if let Ok("release") = std::env::var("PROFILE").as_deref() {
        let frontend = std::env::var(GRAFBABE_FRONTEND).unwrap_or("frontend".into());
        println!("cargo::rerun-if-changed={frontend}");
        println!("cargo::rerun-if-env-changed={GRAFBABE_FRONTEND}")
    }
}
