//! Compiles the Arduino library's protocol core (firmware/faberlink/src/FaberLinkCore.cpp) and the
//! fake-board harness for the PC: MSVC on Windows, g++ or clang on Linux and macOS. The core is
//! built as strict C++11 with warnings as errors, no exceptions and no RTTI, as on a board.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib = root.join("../../firmware/faberlink/src");
    let harness = root.join("harness");
    for f in ["FaberLinkCore.cpp", "FaberLinkCore.h"] {
        println!("cargo:rerun-if-changed={}", lib.join(f).display());
    }
    println!("cargo:rerun-if-changed={}", harness.join("harness.cpp").display());

    let mut build = cc::Build::new();
    build.cpp(true).include(&lib).file(lib.join("FaberLinkCore.cpp")).file(harness.join("harness.cpp"));
    build.warnings(true).warnings_into_errors(true);
    if build.get_compiler().is_like_msvc() {
        // MSVC has no C++11 mode; C++14 is its oldest.
        build.std("c++14").flag("/W4").flag("/GR-");
    } else {
        build.std("c++11").flag("-Wextra").flag("-pedantic").flag("-fno-exceptions").flag("-fno-rtti");
    }
    build.compile("faberlink_harness");
}
