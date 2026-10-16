//! The FaberLink Arduino library and the sketch templates, built into the core.
//!
//! Every sketch is compiled with this copy (`--library`), so a sketch always speaks the FaberLink
//! version of the core that compiled it, whatever library the Arduino IDE has in its own folder.

use std::io;
use std::path::{Path, PathBuf};

macro_rules! library {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("../../../firmware/faberlink/", $path)))),*]
    };
}

/// Every file of `firmware/faberlink`, by its path in the library. A test checks that none is
/// missing.
pub const FILES: &[(&str, &str)] = library![
    "library.properties",
    "src/FaberLink.h",
    "src/FaberLink.cpp",
    "src/FaberLinkCore.h",
    "src/FaberLinkCore.cpp",
    "src/FaberServo.h",
    "src/FaberEncoder.h",
    "src/FaberImu.h",
    "src/FaberToF.h",
    "examples/MotorAndDistance/MotorAndDistance.ino",
    "examples/Ultrasonic/Ultrasonic.ino",
    "examples/BatteryMonitor/BatteryMonitor.ino",
    "examples/MotorL298N/MotorL298N.ino",
    "examples/ButtonAndBumper/ButtonAndBumper.ino",
    "examples/LedAndRelay/LedAndRelay.ino",
    "examples/ServoArm/ServoArm.ino",
    "examples/WheelEncoder/WheelEncoder.ino",
    "examples/KnobAndJoystick/KnobAndJoystick.ino",
    "examples/Mpu6050/Mpu6050.ino",
];

/// Writes the library to `<dir>/FaberLink/` (only the files that differ) and returns that folder.
pub fn install(dir: &Path) -> io::Result<PathBuf> {
    let root = dir.join("FaberLink");
    for (rel, text) in FILES {
        let path = root.join(rel);
        if std::fs::read_to_string(&path).is_ok_and(|t| t == *text) {
            continue;
        }
        std::fs::create_dir_all(path.parent().expect("a file in a folder"))?;
        std::fs::write(&path, text)?;
    }
    Ok(root)
}

/// A sketch to start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// It uses the FaberLink library: the board shows up in Parts.
    pub faberlink: bool,
    /// The example it is (`examples/<example>/<example>.ino`), or `None` for one written here.
    example: Option<&'static str>,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "blank",
        name: "Empty sketch",
        description: "setup() and loop(), nothing else.",
        faberlink: false,
        example: None,
    },
    Template {
        id: "faberlink-board",
        name: "FaberLink board",
        description: "A board Faber talks to: add your motors and sensors to it.",
        faberlink: true,
        example: None,
    },
    Template {
        id: "motor-and-distance",
        name: "FaberLink: motor and distance sensor",
        description: "A DC motor on an H-bridge and a Sharp infrared distance sensor.",
        faberlink: true,
        example: Some("MotorAndDistance"),
    },
    Template {
        id: "motor-l298n",
        name: "FaberLink: two motors on an L298N",
        description: "Two DC motors on an L298N (or TB6612) motor driver.",
        faberlink: true,
        example: Some("MotorL298N"),
    },
    Template {
        id: "ultrasonic",
        name: "FaberLink: ultrasonic distance (HC-SR04)",
        description: "An HC-SR04 range finder on pins 7 and 6.",
        faberlink: true,
        example: Some("Ultrasonic"),
    },
    Template {
        id: "button-and-bumper",
        name: "FaberLink: a button and a bumper",
        description: "A push button on pin 4 and a bumper switch on pin 5, both to GND.",
        faberlink: true,
        example: Some("ButtonAndBumper"),
    },
    Template {
        id: "led-and-relay",
        name: "FaberLink: an LED and a relay",
        description: "A dimmable LED on pin 5 and a relay module on pin 7.",
        faberlink: true,
        example: Some("LedAndRelay"),
    },
    Template {
        id: "servo-arm",
        name: "FaberLink: a servo",
        description: "A hobby servo (SG90) on pin 9, on its own supply.",
        faberlink: true,
        example: Some("ServoArm"),
    },
    Template {
        id: "wheel-encoder",
        name: "FaberLink: a gear motor with an encoder",
        description: "A motor on a PWM + DIR driver, measured by a hall encoder on pins 2 and 3.",
        faberlink: true,
        example: Some("WheelEncoder"),
    },
    Template {
        id: "knob-and-joystick",
        name: "FaberLink: a knob and a joystick",
        description: "A potentiometer on A0, and a thumb joystick on A1, A2 and pin 4.",
        faberlink: true,
        example: Some("KnobAndJoystick"),
    },
    Template {
        id: "mpu6050",
        name: "FaberLink: an IMU (MPU6050)",
        description: "A GY-521 board on the board's I²C pins.",
        faberlink: true,
        example: Some("Mpu6050"),
    },
    Template {
        id: "battery-monitor",
        name: "FaberLink: battery monitor",
        description: "A battery's voltage through a divider on A1.",
        faberlink: true,
        example: Some("BatteryMonitor"),
    },
];

impl Template {
    pub fn by_id(id: &str) -> Option<&'static Template> {
        TEMPLATES.iter().find(|t| t.id == id)
    }

    /// The `.ino` for a sketch called `name`.
    pub fn text(&self, name: &str) -> String {
        if let Some(example) = self.example {
            let path = format!("examples/{example}/{example}.ino");
            return FILES
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, t)| t.to_string())
                .expect("listed in FILES");
        }
        match self.id {
            "faberlink-board" => format!(
                "// {name}: a board Faber talks to over FaberLink (docs/connect-an-arduino.md).\n\
                 //\n\
                 // Add a part for each motor and sensor, e.g.\n\
                 //   FaberMotor left(\"left\", 9, 8);    // a motor driver: PWM on pin 9, direction on pin 8\n\
                 //   FaberMotor right(\"right\", faberlink::DRIVER_EN_IN1_IN2, 10, 7, 6); // an L298N: EN, IN1, IN2\n\
                 //   FaberDistance front(\"front\", A0); // an analog distance sensor on A0\n\
                 // The library's examples (New sketch) show every kind of part.\n\
                 //\n\
                 // Don't print to Serial: the port belongs to FaberLink. Use link.log(\"...\") instead.\n\
                 \n\
                 #include <FaberLink.h>\n\
                 \n\
                 FaberLink link(Serial);\n\
                 \n\
                 void setup() {{\n\
                 \x20 // The board's name and the sketch's version; Faber shows both.\n\
                 \x20 link.begin(115200, \"{board}\", \"0.1.0\");\n\
                 }}\n\
                 \n\
                 void loop() {{\n\
                 \x20 link.poll();  // the library does the rest; keep loop() free of delay()\n\
                 }}\n",
                board = name.to_ascii_lowercase()
            ),
            _ => format!(
                "// {name}\n\
                 \n\
                 void setup() {{\n\
                 \x20 // Runs once, when the board starts.\n\
                 }}\n\
                 \n\
                 void loop() {{\n\
                 \x20 // Runs over and over.\n\
                 }}\n"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files_in(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if e.path().is_dir() {
                files_in(&e.path(), &rel, out);
            } else {
                out.push(rel);
            }
        }
    }

    #[test]
    fn every_file_of_the_library_is_built_in() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../firmware/faberlink");
        let mut on_disk = Vec::new();
        files_in(&dir, "", &mut on_disk);
        on_disk.sort();
        let mut built_in: Vec<String> = FILES.iter().map(|(p, _)| p.to_string()).collect();
        built_in.sort();
        assert_eq!(built_in, on_disk, "add the new file to FILES in core/src/arduino/library.rs");
    }

    #[test]
    fn installing_writes_the_library_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = install(dir.path()).unwrap();
        assert_eq!(root, dir.path().join("FaberLink"));
        let h = root.join("src/FaberLinkCore.h");
        assert!(std::fs::read_to_string(&h).unwrap().contains("setBuildId"));
        let before = std::fs::metadata(&h).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        install(dir.path()).unwrap();
        assert_eq!(
            std::fs::metadata(&h).unwrap().modified().unwrap(),
            before,
            "an unchanged file isn't rewritten"
        );
    }

    #[test]
    fn templates_name_the_sketch_and_the_examples_are_the_librarys() {
        let t = Template::by_id("faberlink-board").unwrap().text("Rover2");
        assert!(t.starts_with("// Rover2: a board"), "{t}");
        assert!(t.contains("link.begin(115200, \"rover2\", \"0.1.0\");"));
        let m = Template::by_id("motor-l298n").unwrap().text("Anything");
        assert!(m.contains("FaberMotor left(\"left\", faberlink::DRIVER_EN_IN1_IN2, 10, 8, 7);"));
        let u = Template::by_id("ultrasonic").unwrap().text("Anything");
        assert!(u.contains("FaberUltrasonic front(\"front\", 7, 6);"));
        assert!(Template::by_id("blank").unwrap().text("X").contains("void loop() {"));
        assert_eq!(Template::by_id("nope"), None);
        let examples = FILES.iter().filter(|(p, _)| p.ends_with(".ino")).count();
        assert_eq!(
            TEMPLATES.iter().filter(|t| t.example.is_some()).count(),
            examples,
            "one template per example"
        );
    }
}
