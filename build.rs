fn main() {
    std::env::set_var("QT_VERSION_MAJOR", "6");
    if std::process::Command::new("qmake6")
        .arg("-query")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        std::env::set_var("QMAKE", "qmake6");
    }

    println!("cargo:rerun-if-changed=src/qt-rs/live_dock.rs");
    println!("cargo:rerun-if-changed=src/qt/live_dock.cpp");
    println!("cargo:rerun-if-changed=build.rs");

    cxx_qt_build::CxxQtBuilder::new()
        .include_prefix("obs_chzzk_extension")
        .file("src/qt-rs/live_dock.rs")
        .qt_module("Gui")
        .qt_module("Widgets")
        .qt_module("Network")
        .cc_builder(|cc| {
            cc.file("src/qt/live_dock.cpp");
        })
        .build();

    println!("cargo:rustc-link-lib=dylib=stdc++");
}
