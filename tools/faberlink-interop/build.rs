//! Compiles the Arduino library's protocol core (firmware/faberlink/src/FaberLinkCore.cpp) and the
//! fake-board harness for the PC: MSVC on Windows, g++ or clang on Linux and macOS. The core is
//! built as strict C++11 with warnings as errors, no exceptions and no RTTI, as on a board.
//!
//! The library's Arduino glue (FaberLink.cpp) is built too, against the stand-in Arduino core in
//! `harness/arduino/`, so a whole sketch's parts can be checked pin by pin (`harness/glue.cpp`).

use std::path::{Path, PathBuf};

fn strict(build: &mut cc::Build) {
    build.cpp(true).warnings(true).warnings_into_errors(true);
    if build.get_compiler().is_like_msvc() {
        // MSVC has no C++11 mode; C++14 is its oldest.
        build.std("c++14").flag("/W4").flag("/GR-");
    } else {
        build.std("c++11").flag("-Wextra").flag("-pedantic").flag("-fno-exceptions").flag("-fno-rtti");
    }
}

fn watch(dir: &Path, files: &[&str]) {
    for f in files {
        println!("cargo:rerun-if-changed={}", dir.join(f).display());
    }
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib = root.join("../../firmware/faberlink/src");
    let harness = root.join("harness");
    let arduino = harness.join("arduino");
    watch(&lib, &["FaberLinkCore.cpp", "FaberLinkCore.h", "FaberLink.cpp", "FaberLink.h", "FaberServo.h"]);
    watch(&harness, &["harness.cpp", "glue.cpp", "arduino/Arduino.h", "arduino/Servo.h"]);

    let mut build = cc::Build::new();
    strict(&mut build);
    build.include(&lib).file(lib.join("FaberLinkCore.cpp")).file(harness.join("harness.cpp"));
    build.compile("faberlink_harness");

    // The glue as an AVR board compiles it (ARDUINO_ARCH_AVR: A0 is the analog base).
    let mut glue = cc::Build::new();
    strict(&mut glue);
    if glue.get_compiler().is_like_msvc() {
        // glue.cpp is test code and uses the standard library's containers.
        glue.flag("/EHsc");
    }
    glue.include(&arduino)
        .include(&lib)
        .define("ARDUINO_ARCH_AVR", None)
        .file(lib.join("FaberLink.cpp"))
        .file(harness.join("glue.cpp"));
    glue.compile("faberlink_glue");
}
