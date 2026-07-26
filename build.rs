const GRAFBABE_FRONTEND: &'static str = "GRAFBABE_FRONTEND";

fn main() {
    if let Ok("release") = std::env::var("PROFILE").as_deref() {
        let frontend = std::env::var(GRAFBABE_FRONTEND).unwrap_or("frontend".into());
        println!("cargo::rerun-if-changed={frontend}");
        println!("cargo::rerun-if-env-changed={GRAFBABE_FRONTEND}")
    }

    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(var, _)| {
            var.strip_prefix("CARGO_FEATURE_")
                .map(|s| s.to_ascii_lowercase().to_string())
        })
        .filter(|feature| feature != "default")
        .collect();
    features.sort();
    let features = features.join(" ");
    println!("cargo::rustc-env=GRAFBABE_FEATURES={features}");
}
