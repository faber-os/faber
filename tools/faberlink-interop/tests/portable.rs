//! The Arduino library's protocol core must stay portable and small (docs/faberlink.md §5): no
//! Arduino headers, no dynamic allocation, no exceptions. The interop build compiles it with
//! `-fno-exceptions -fno-rtti`; this test also reads the source, so a `new` or a `malloc` can't
//! slip in through code the tests happen not to reach.

use std::path::PathBuf;

fn core_sources() -> Vec<(String, String)> {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../firmware/faberlink/src");
    ["FaberLinkCore.h", "FaberLinkCore.cpp"]
        .iter()
        .map(|f| (f.to_string(), std::fs::read_to_string(src.join(f)).expect("the core source")))
        .collect()
}

/// The code without `//` comments, so that the rules can be explained in comments.
fn code(source: &str) -> String {
    source.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

#[test]
fn the_protocol_core_has_no_arduino_headers_allocation_or_exceptions() {
    let forbidden = [
        ("Arduino.h", "an Arduino header"),
        ("#include <Arduino", "an Arduino header"),
        ("malloc(", "dynamic allocation"),
        ("calloc(", "dynamic allocation"),
        ("realloc(", "dynamic allocation"),
        ("free(", "dynamic allocation"),
        (" new ", "dynamic allocation"),
        ("new(", "dynamic allocation"),
        ("delete ", "dynamic allocation"),
        ("throw", "exceptions"),
        ("try {", "exceptions"),
        ("catch", "exceptions"),
        ("dynamic_cast", "RTTI"),
        ("typeid", "RTTI"),
        ("#include <string>", "the C++ standard library"),
        ("#include <vector>", "the C++ standard library"),
        ("std::", "the C++ standard library"),
    ];
    for (file, source) in core_sources() {
        let code = code(&source);
        for (needle, what) in forbidden {
            assert!(!code.contains(needle), "{file} uses {what} (`{needle}`)");
        }
    }
}

#[test]
fn the_arduino_glue_is_separate_from_the_core() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../firmware/faberlink/src");
    let glue = std::fs::read_to_string(src.join("FaberLink.h")).unwrap();
    assert!(glue.contains("#include <Arduino.h>"));
    assert!(glue.contains("#include \"FaberLinkCore.h\""));
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../firmware/faberlink/examples/MotorAndDistance/MotorAndDistance.ino");
    assert!(std::fs::read_to_string(example).unwrap().contains("link.poll()"));
}
