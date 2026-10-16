//! The compiler's complaints as a list: file, line, column and message (docs/arduino-editor.md).
//!
//! `arduino-cli` passes on avr-gcc's output, which points into the sketch's own files (the IDE's
//! `#line` markers map the generated `.ino.cpp` back to the `.ino`):
//!
//! ```text
//! C:\…\Blink\Blink.ino:5:1: error: expected ',' or ';' before '}' token
//! C:\…\Blink\helper.h:2:30: error: 'nope' was not declared in this scope
//! ```
//!
//! Errors anywhere are kept (an error in a library still stops the build). Warnings are kept only
//! for the sketch's own files: the Arduino core's warnings are not the user's to fix.

/// How bad it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// For a file in the sketch, its path in the sketch folder (`Blink.ino`, `src/motor.cpp`);
    /// for another file (a library's), its name. `None` when the compiler names no file.
    pub file: Option<String>,
    /// True when `file` is one of the sketch's own files, so the editor can open it at the line.
    pub in_sketch: bool,
    /// From 1.
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}

const MARKERS: &[(&str, Severity)] = &[
    (": fatal error: ", Severity::Error),
    (": error: ", Severity::Error),
    (": warning: ", Severity::Warning),
];

fn normal(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

/// Splits `path:line:column` or `path:line`; a Windows drive's `C:` is never a line.
fn location(loc: &str) -> (&str, Option<u32>, Option<u32>) {
    let num =
        |s: &str| (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten();
    if let Some((rest, last)) = loc.rsplit_once(':')
        && let Some(n) = num(last)
    {
        if let Some((path, line)) = rest.rsplit_once(':')
            && let Some(l) = num(line)
            && !path.is_empty()
        {
            return (path, Some(l), Some(n));
        }
        if !rest.is_empty() {
            return (rest, Some(n), None);
        }
    }
    (loc, None, None)
}

/// The file as the editor shows it: relative to the sketch folder, or just its name.
fn file_of(path: &str, sketch_dir: &str) -> (Option<String>, bool) {
    let p = path.replace('\\', "/");
    let lower = p.to_ascii_lowercase();
    let dir = sketch_dir.trim_end_matches('/');
    if !dir.is_empty() && lower.starts_with(dir) && lower[dir.len()..].starts_with('/') {
        return (Some(p[dir.len() + 1..].to_string()), true);
    }
    let name = p.rsplit('/').next().unwrap_or(&p);
    // `collect2`, `ld.exe` and the like are tools, not files.
    if name.is_empty() || !name.contains('.') || name.ends_with(".exe") {
        return (None, false);
    }
    (Some(name.to_string()), false)
}

/// Every error and sketch warning in the compiler's output. `sketch_dir` is the folder the
/// sketch was compiled from.
pub fn parse(output: &str, sketch_dir: &std::path::Path) -> Vec<Diagnostic> {
    let dir = normal(&sketch_dir.to_string_lossy());
    let mut out: Vec<Diagnostic> = Vec::new();
    for raw in output.lines() {
        let line = raw.trim_end();
        let found = MARKERS
            .iter()
            .filter_map(|(m, sev)| line.find(m).filter(|&i| i > 1).map(|i| (i, *m, *sev)))
            .min_by_key(|(i, _, _)| *i);
        let d = if let Some((i, marker, severity)) = found {
            let message = line[i + marker.len()..].trim().to_string();
            if message.starts_with("ld returned") {
                continue; // collect2's summary of the linker errors already listed
            }
            let (path, line_no, column) = location(&line[..i]);
            let (file, in_sketch) = file_of(path, &dir);
            Diagnostic { severity, file, in_sketch, line: line_no, column, message }
        } else if let Some(i) = line.find("undefined reference to ") {
            // A linker error: `…/Blink.ino:8: undefined reference to `foo'`, or with no line.
            let (path, line_no, _) = location(line[..i].trim_end().trim_end_matches(':'));
            let (file, in_sketch) = file_of(path, &dir);
            let (line_no, file) = if in_sketch { (line_no, file) } else { (None, None) };
            Diagnostic {
                severity: Severity::Error,
                file,
                in_sketch,
                line: line_no,
                column: None,
                message: line[i..].trim().to_string(),
            }
        } else {
            continue;
        };
        if d.severity == Severity::Warning && !d.in_sketch {
            continue;
        }
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// What arduino-cli 1.5.1 printed on Windows for a sketch with a broken tab and two errors in
    /// the .ino (from the manual check in docs/arduino-editor.md), with the path shortened.
    const REAL: &str = "In file included from C:\\Work\\Bad\\Bad.ino:1:0:\r
C:\\Work\\Bad\\helper.h: In function 'int helper()':\r
C:\\Work\\Bad\\helper.h:2:30: error: 'nope' was not declared in this scope\r
 inline int helper() { return nope; }\r
                              ^~~~\r
C:\\Work\\Bad\\helper.h:2:30: note: suggested alternative: 'tone'\r
C:\\Work\\Bad\\Bad.ino: In function 'void setup()':\r
C:\\Work\\Bad\\Bad.ino:5:1: error: expected ',' or ';' before '}' token\r
 }\r
 ^\r
C:\\Work\\Bad\\Bad.ino: In function 'void loop()':\r
C:\\Work\\Bad\\Bad.ino:8:3: error: 'undefinedCall' was not declared in this scope\r
   undefinedCall();\r
   ^~~~~~~~~~~~~\r
";

    fn err(file: &str, line: u32, column: u32, message: &str) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            file: Some(file.into()),
            in_sketch: true,
            line: Some(line),
            column: Some(column),
            message: message.into(),
        }
    }

    #[test]
    fn real_errors_become_file_line_and_message() {
        let d = parse(REAL, Path::new("C:\\Work\\Bad"));
        assert_eq!(
            d,
            [
                err("helper.h", 2, 30, "'nope' was not declared in this scope"),
                err("Bad.ino", 5, 1, "expected ',' or ';' before '}' token"),
                err("Bad.ino", 8, 3, "'undefinedCall' was not declared in this scope"),
            ]
        );
    }

    #[test]
    fn unix_paths_and_src_folders_work_too() {
        let out = "/home/pi/w/Robot/src/motor.cpp:12:5: warning: unused variable 'x' [-Wunused-variable]\n\
                   /home/pi/w/Robot/Robot.ino:3: error: stray '\\302' in program\n";
        let d = parse(out, Path::new("/home/pi/w/Robot"));
        assert_eq!(d[0].file.as_deref(), Some("src/motor.cpp"));
        assert_eq!((d[0].severity, d[0].line, d[0].column), (Severity::Warning, Some(12), Some(5)));
        assert_eq!((d[1].file.as_deref(), d[1].line, d[1].column), (Some("Robot.ino"), Some(3), None));
    }

    #[test]
    fn the_arduino_cores_warnings_are_left_out_but_library_errors_stay() {
        let out = "C:\\A15\\cores\\arduino\\new.cpp:59:60: warning: unused parameter 'tag' [-Wunused-parameter]\n\
                   C:\\Faber\\libraries\\FaberLink\\src\\FaberLink.h:12:3: error: 'Foo' does not name a type\n\
                   C:\\Work\\Bad\\Bad.ino:1:10: fatal error: Servo2.h: No such file or directory\n";
        let d = parse(out, Path::new("C:\\Work\\Bad"));
        assert_eq!(d.len(), 2, "{d:?}");
        assert_eq!((d[0].file.as_deref(), d[0].in_sketch), (Some("FaberLink.h"), false));
        assert_eq!(d[1].message, "Servo2.h: No such file or directory");
        assert_eq!(d[1].severity, Severity::Error);
    }

    #[test]
    fn linker_errors_are_listed_without_a_file_when_none_is_known() {
        let out = "C:\\Temp\\ccX.ltrans0.ltrans.o: In function `loop':\n\
                   <artificial>:(.text+0x40): undefined reference to `motorGo()'\n\
                   collect2.exe: error: ld returned 1 exit status\n";
        let d = parse(out, Path::new("C:\\Work\\Bad"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].message, "undefined reference to `motorGo()'");
        assert_eq!((d[0].file.as_deref(), d[0].line), (None, None));
    }

    #[test]
    fn the_same_complaint_twice_is_listed_once() {
        let out = "/w/B/B.ino:2:1: error: x\n/w/B/B.ino:2:1: error: x\n";
        assert_eq!(parse(out, Path::new("/w/B")).len(), 1);
    }
}
