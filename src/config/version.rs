pub static NAME: &'static str = env!("CARGO_CRATE_NAME");
static VERSION: &'static str = env!("CARGO_PKG_VERSION");
static FEATURES: &'static str = env!("GRAFBABE_FEATURES");

pub fn version() {
    println!("{NAME} {VERSION}");
}

pub fn version_more() {
    println!("{NAME} {VERSION}");
    println!("Compiled with features:");
    for feature in FEATURES.split(' ') {
        println!("\t{feature}");
    }

    #[cfg(not(debug_assertions))]
    {
        println!("Included frontend files:");
        let root = crate::serve_http::INCLUDED_FILES_ROOT;
        for (filename, _) in crate::serve_http::INCLUDED_FILES {
            println!("\t{root}/{filename}");
        }
    }
}
