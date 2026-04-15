fn main() {
    println!("cargo:rerun-if-changed=src/qt/live_dock.cpp");
    println!("cargo:rerun-if-changed=build.rs");

    let qt6_widgets = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("Qt6Widgets");
    let qt6_network = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("Qt6Network");

    let (widgets, network) = match (qt6_widgets, qt6_network) {
        (Ok(widgets), Ok(network)) => (widgets, network),
        _ => {
            let widgets = pkg_config::Config::new()
                .cargo_metadata(true)
                .probe("Qt5Widgets")
                .expect("Qt5Widgets development package is required");
            let network = pkg_config::Config::new()
                .cargo_metadata(true)
                .probe("Qt5Network")
                .expect("Qt5Network development package is required");
            (widgets, network)
        }
    };

    let mut build = cc::Build::new();
    build.cpp(true);
    build.file("src/qt/live_dock.cpp");
    build.flag_if_supported("-std=c++17");

    for include_path in widgets.include_paths {
        build.include(include_path);
    }

    for include_path in network.include_paths {
        build.include(include_path);
    }

    build.compile("obs_chzzk_live_dock");

    println!("cargo:rustc-link-lib=dylib=stdc++");
}
