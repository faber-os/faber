//! Guided boards (docs/wiring.md §5–§8): a board whose sketch Faber writes from its plan.
//!
//! A guided board is a sketch folder like any other, `home:/Arduino/<name>/`, with the plan
//! `board.toml`, the generated `<name>.ino` and your own `my_code.h`. Saving a plan checks it with
//! the allocator, writes `board.toml` (keeping its comments) and the sketch, and creates
//! `my_code.h` once. Uploading is the editor's upload, as for any sketch.
//!
//! The sketch's SHA-256 is kept in `board.toml` (`sketch-sha256`), so a sketch changed outside
//! Faber is noticed and never overwritten without being asked. "Take over the code" (§8) is one
//! way: the sketch loses its generated mark and the plan becomes a note.
//!
//! None of this takes the machine's state lock: it only reads and writes files, one plan change
//! at a time ([`super::arduino::Workshop`]'s `plans` lock, a leaf).

use super::arduino::SKETCHBOOK;
use super::{ErrorKind, Machine, MachineError};
use crate::arduino::sketch::{self, sha256_hex};
use crate::arduino::wiring::generate::{self, Wiring};
use crate::arduino::wiring::pins::{self, BoardKind, HeaderPosition, PinMap};
use crate::arduino::wiring::plan::{self, Plan};
use crate::arduino::wiring::{
    BuiltIn, Candidate, ExternalUse, Problem, Severity, Suggestion, Template, allocator,
};
use crate::files::VPath;
use std::path::{Path, PathBuf};

/// The key in `board.toml` that holds the SHA-256 of the sketch Faber last wrote.
const SKETCH_HASH: &str = "sketch-sha256";

/// The note at the top of a taken-over plan.
const TAKEN_OVER_NOTE: &str =
    "# Taken over: the sketch is your code now. Faber no longer reads this plan or changes the sketch.\n";

/// A guided board, as the editor and the wizard show it.
#[derive(Clone, Debug, PartialEq)]
pub struct GuidedBoard {
    pub name: String,
    /// The board's folder in Files and its sketch (`home:/Arduino/mega/mega.ino`).
    pub folder: String,
    pub sketch: String,
    /// The plan, unless `board.toml` can't be read (then `file_problem` says why).
    pub plan: Option<Plan>,
    /// What is wrong with `board.toml` as a file, with its line.
    pub file_problem: Option<Problem>,
    /// What the allocator says about the plan.
    pub problems: Vec<Problem>,
    pub wiring: Vec<Wiring>,
    /// The sketch still carries Faber's mark.
    pub generated: bool,
    /// The sketch was changed outside Faber since Faber wrote it.
    pub edited: bool,
}

/// A guided board in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct GuidedBoardSummary {
    pub name: String,
    pub board: Option<String>,
    pub parts: usize,
    pub taken_over: bool,
    /// `board.toml` can't be read, or the plan has errors.
    pub problem: Option<String>,
}

fn bad(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> MachineError {
    MachineError::new(ErrorKind::BadRequest, code, message, hint)
}

fn internal(what: &str, e: impl std::fmt::Display) -> MachineError {
    MachineError::new(
        ErrorKind::Internal,
        "io-error",
        format!("Faber could not {what}: {e}"),
        "Check that the disk isn't full. The log has details.",
    )
}

/// The value of `sketch-sha256` in a plan's text.
fn recorded_hash(text: &str) -> Option<String> {
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    doc.get(SKETCH_HASH)?.as_str().map(str::to_string)
}

/// The plan's text with `sketch-sha256` set (keeping everything else).
fn with_hash(text: &str, hash: &str) -> String {
    let mut doc = text.parse::<toml_edit::DocumentMut>().expect("Faber just wrote it");
    doc[SKETCH_HASH] = toml_edit::value(hash);
    doc.to_string()
}

/// `board.toml` read with its line numbers; a problem points at its line.
fn read_plan(text: &str) -> Result<Plan, Box<Problem>> {
    plan::parse(text).map_err(|e| {
        Box::new(Problem {
            severity: Severity::Error,
            code: "plan-file-invalid".into(),
            part: None,
            role: None,
            setting: None,
            pin: None,
            message: match e.line {
                Some(l) => format!("board.toml, line {l}: {}", e.message),
                None => format!("board.toml: {}", e.message),
            },
            hint: e.hint,
            line: e.line,
        })
    })
}

impl Machine {
    /// A board's pin map (`GET /api/v0/arduino/pins/{board}`): an Arduino's, or the Pi's header.
    pub fn pin_map(&self, board: &str) -> Result<PinMap, MachineError> {
        if board == pins::PI_HEADER_ID {
            return Ok(pins::pi_header(crate::ports::header::HEADER.iter().map(|p| HeaderPosition {
                physical: p.pin,
                name: p.name,
                line: p.line,
            })));
        }
        pins::arduino(board).ok_or_else(|| {
            let known: Vec<&str> = pins::arduino_boards().chain([pins::PI_HEADER_ID]).collect();
            MachineError::new(
                ErrorKind::NotFound,
                "unknown-board",
                format!("Faber has no pin map for the board \"{board}\"."),
                format!("Pick one of: {}.", known.join(", ")),
            )
        })
    }

    /// The part templates that can be used now (`GET /api/v0/arduino/parts`): not one whose every
    /// shape still waits for the library (a VL53L0X needs a library Faber can't install yet).
    pub fn part_templates(&self) -> Vec<Template> {
        crate::arduino::wiring::parts::templates().iter().filter(|t| !t.shapes.is_empty()).cloned().collect()
    }

    /// Pins in use on the board from outside its plan. Empty for now: a guided board's own
    /// sketch reports exactly its plan's pins. When code-mode boards report theirs (the `PINS`
    /// message, docs/wiring.md §6), "Start a guided plan for this board" feeds them in here.
    fn pins_used_elsewhere(&self, _plan: &Plan) -> Vec<ExternalUse> {
        Vec::new()
    }

    fn sketchbook_dir(&self) -> Result<PathBuf, MachineError> {
        Ok(self.files.local_path(&VPath::home(&[SKETCHBOOK]))?)
    }

    fn plan_folder(&self, name: &str) -> Result<(VPath, PathBuf), MachineError> {
        sketch::check_name(name).map_err(|why| {
            bad("invalid-board-name", why, "Try a name like mega or rover: letters, digits, _ and -.")
        })?;
        let folder = VPath::home(&[SKETCHBOOK, name]);
        let local = self.files.local_path(&folder)?;
        Ok((folder, local))
    }

    /// The plan's folder, which must be a guided board's.
    fn existing_plan(&self, name: &str) -> Result<(VPath, PathBuf, String), MachineError> {
        let (folder, local) = self.plan_folder(name)?;
        let text = std::fs::read_to_string(local.join(plan::FILE)).map_err(|_| {
            MachineError::new(
                ErrorKind::NotFound,
                "unknown-plan",
                format!("There is no guided board called {name}."),
                "GET /api/v0/arduino/plans lists them; a sketch without board.toml is in code mode.",
            )
        })?;
        Ok((folder, local, text))
    }

    fn arduino_map(&self, board: &str) -> Result<PinMap, MachineError> {
        let map = self.pin_map(board).map_err(|e| MachineError { kind: ErrorKind::Unprocessable, ..e })?;
        if map.kind != BoardKind::Arduino {
            return Err(MachineError::new(
                ErrorKind::Unprocessable,
                "board-not-guided",
                format!("Guided plans for the {} aren't built yet.", map.name),
                "Pick an Arduino: uno, nano, nano-old or mega2560.",
            ));
        }
        Ok(map)
    }

    fn view(&self, name: &str, folder: &VPath, local: &Path, text: &str) -> GuidedBoard {
        let ino_name = format!("{name}.ino");
        let sketch = folder.join(&ino_name).to_string();
        let ino = std::fs::read_to_string(local.join(&ino_name)).ok();
        let generated = ino.as_deref().is_some_and(generate::is_generated);
        let edited = match (&ino, recorded_hash(text)) {
            (Some(ino), Some(hash)) => sha256_hex(ino.as_bytes()) != hash,
            // No record: a plan written by hand. Only a marked sketch is Faber's.
            (Some(_), None) => false,
            (None, _) => false,
        };
        let mut out = GuidedBoard {
            name: name.into(),
            folder: folder.to_string(),
            sketch,
            plan: None,
            file_problem: None,
            problems: Vec::new(),
            wiring: Vec::new(),
            generated,
            edited,
        };
        match read_plan(text) {
            Err(p) => out.file_problem = Some(*p),
            Ok(plan) => {
                if let Ok(map) = self.pin_map(&plan.board) {
                    let plan = plan.normalized(&map, &BuiltIn);
                    if plan.taken_over.is_none() {
                        out.problems =
                            allocator::check(&map, &plan, &BuiltIn, &self.pins_used_elsewhere(&plan));
                        for p in &mut out.problems {
                            p.line = locate(text, p);
                        }
                    }
                    out.wiring = generate::wiring(&plan, &map, &BuiltIn);
                    out.plan = Some(plan);
                } else {
                    out.file_problem = Some(Problem {
                        severity: Severity::Error,
                        code: "unknown-board".into(),
                        part: None,
                        role: None,
                        setting: None,
                        pin: None,
                        message: format!(
                            "board.toml names the board \"{}\", which Faber has no pin map for.",
                            plan.board
                        ),
                        hint: "Set board to uno, nano, nano-old or mega2560.".into(),
                        line: plan::span_of(text, &["board"]).map(|s| plan::line_col(text, s.start).0),
                    });
                    out.plan = Some(plan);
                }
            }
        }
        out
    }

    /// Every guided board in the sketchbook (`GET /api/v0/arduino/plans`), by name.
    pub fn guided_boards(&self) -> Result<Vec<GuidedBoardSummary>, MachineError> {
        let book = self.sketchbook_dir()?;
        let Ok(entries) = std::fs::read_dir(&book) else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(text) = std::fs::read_to_string(e.path().join(plan::FILE)) else { continue };
            if sketch::check_name(&name).is_err() {
                continue;
            }
            let folder = VPath::home(&[SKETCHBOOK, &name]);
            let v = self.view(&name, &folder, &e.path(), &text);
            let problem = v.file_problem.as_ref().map(|p| p.message.clone()).or_else(|| {
                v.problems.iter().find(|p| p.severity == Severity::Error).map(|p| p.message.clone())
            });
            out.push(GuidedBoardSummary {
                name,
                board: v.plan.as_ref().map(|p| p.board.clone()),
                parts: v.plan.as_ref().map_or(0, |p| p.parts.len()),
                taken_over: v.plan.as_ref().is_some_and(|p| p.taken_over.is_some()),
                problem,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// One guided board (`GET /api/v0/arduino/plans/{name}`).
    pub fn guided_board(&self, name: &str) -> Result<GuidedBoard, MachineError> {
        let (folder, local, text) = self.existing_plan(name)?;
        Ok(self.view(name, &folder, &local, &text))
    }

    /// Starts a guided board (`POST /api/v0/arduino/plans`): a new sketch folder with an empty
    /// plan, its sketch and `my_code.h`.
    pub fn new_guided_board(&self, name: &str, board: &str) -> Result<GuidedBoard, MachineError> {
        let map = self.arduino_map(board)?;
        let _plans = self.arduino.plans.lock().expect("plans lock");
        let (folder, local) = self.plan_folder(name)?;
        if local.exists() {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "already-exists",
                format!("There is already a sketch called {name}."),
                "Pick another name for the board.",
            ));
        }
        std::fs::create_dir_all(&local).map_err(|e| internal("make the board's folder", e))?;
        let plan = Plan::new(board, name);
        let text = self.write_plan_files(&local, None, &plan, &map)?;
        self.notify_folders(&[VPath::home(&[SKETCHBOOK]), folder.clone()]);
        tracing::info!(target: "core::arduino", "new guided board {name} ({})", map.name);
        Ok(self.view(name, &folder, &local, &text))
    }

    /// Writes the sketch, then the plan with the sketch's hash, and `my_code.h` if it is
    /// missing. Returns the plan's text.
    fn write_plan_files(
        &self,
        local: &Path,
        old: Option<&str>,
        plan: &Plan,
        map: &PinMap,
    ) -> Result<String, MachineError> {
        let ino = generate::sketch(plan, map, &BuiltIn).map_err(invalid_plan)?;
        std::fs::write(local.join(format!("{}.ino", plan.name)), &ino)
            .map_err(|e| internal("write the sketch", e))?;
        let text = with_hash(&plan::write(old, plan, map, &BuiltIn), &sha256_hex(ino.as_bytes()));
        std::fs::write(local.join(plan::FILE), &text).map_err(|e| internal("write board.toml", e))?;
        let mine = local.join(generate::MY_CODE);
        if !mine.exists() {
            std::fs::write(&mine, generate::my_code(plan)).map_err(|e| internal("write my_code.h", e))?;
        }
        Ok(text)
    }

    /// Where a candidate part could go (`POST /api/v0/arduino/plans/{name}/suggest`). Writes
    /// nothing.
    pub fn suggest_part(&self, name: &str, candidate: &Candidate) -> Result<Suggestion, MachineError> {
        let (_, _, text) = self.existing_plan(name)?;
        let plan = read_plan(&text).map_err(|p| plan_file_error(name, *p))?;
        let map = self.arduino_map(&plan.board)?;
        let plan = plan.normalized(&map, &BuiltIn);
        allocator::suggest(&map, &plan, &BuiltIn, &self.pins_used_elsewhere(&plan), candidate).map_err(|p| {
            MachineError::new(ErrorKind::BadRequest, "invalid-part", p.message.clone(), p.hint.clone())
        })
    }

    /// Saves a plan and writes its sketch (`PUT /api/v0/arduino/plans/{name}`). Refuses a plan
    /// with errors (`invalid-plan`, every problem listed), a taken-over board, and a sketch
    /// changed outside Faber unless `overwrite_edits`.
    pub fn save_plan(
        &self,
        name: &str,
        plan: Plan,
        overwrite_edits: bool,
    ) -> Result<GuidedBoard, MachineError> {
        let _plans = self.arduino.plans.lock().expect("plans lock");
        let (folder, local, old) = self.existing_plan(name)?;
        if let Ok(current) = read_plan(&old)
            && current.taken_over.is_some()
        {
            return Err(taken_over_error(name));
        }
        let ino_path = local.join(format!("{name}.ino"));
        if let Ok(ino) = std::fs::read_to_string(&ino_path) {
            if !generate::is_generated(&ino) {
                return Err(taken_over_error(name));
            }
            let edited = recorded_hash(&old).is_some_and(|h| h != sha256_hex(ino.as_bytes()));
            if edited && !overwrite_edits {
                return Err(MachineError::new(
                    ErrorKind::Conflict,
                    "sketch-edited",
                    format!("{name}.ino was changed outside Faber since Faber wrote it."),
                    "Take over the code to keep your changes, or save again with overwriteEdits to replace them with the plan's sketch.",
                ));
            }
        }
        if plan.taken_over.is_some() {
            return Err(bad(
                "invalid-plan",
                "A plan can't be saved as taken over.",
                "Use POST /api/v0/arduino/plans/{name}/take-over.",
            ));
        }
        let map = self.arduino_map(&plan.board)?;
        let plan = Plan { name: name.into(), ..plan }.normalized(&map, &BuiltIn);
        let errors: Vec<Problem> = allocator::check(&map, &plan, &BuiltIn, &self.pins_used_elsewhere(&plan))
            .into_iter()
            .filter(|p| p.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(invalid_plan(errors));
        }
        let text = self.write_plan_files(&local, Some(&old), &plan, &map)?;
        self.notify_folders(std::slice::from_ref(&folder));
        tracing::info!(target: "core::arduino", "saved the plan of {name}: {} parts", plan.parts.len());
        Ok(self.view(name, &folder, &local, &text))
    }

    /// "Take over the code" (`POST /api/v0/arduino/plans/{name}/take-over`, docs/wiring.md §8):
    /// the sketch loses its generated mark and is the user's from now on; `board.toml` stays as
    /// a note. One way. Doing it again changes nothing.
    pub fn take_over_plan(&self, name: &str) -> Result<GuidedBoard, MachineError> {
        let _plans = self.arduino.plans.lock().expect("plans lock");
        let (folder, local, old) = self.existing_plan(name)?;
        let mut plan = read_plan(&old).map_err(|p| plan_file_error(name, *p))?;
        if plan.taken_over.is_some() {
            return Ok(self.view(name, &folder, &local, &old));
        }
        let when = format!("{:.0}", jiff::Timestamp::now());
        let ino_path = local.join(format!("{name}.ino"));
        if let Ok(ino) = std::fs::read_to_string(&ino_path)
            && generate::is_generated(&ino)
        {
            let day = when.get(..10).unwrap_or(&when);
            std::fs::write(&ino_path, generate::taken_over(&ino, name, day))
                .map_err(|e| internal("rewrite the sketch", e))?;
        }
        plan.taken_over = Some(when);
        let map = self.pin_map(&plan.board).unwrap_or_else(|_| pins::arduino("uno").expect("the Uno's map"));
        let mut text = plan::write(Some(&old), &plan, &map, &BuiltIn);
        if !text.starts_with(TAKEN_OVER_NOTE) {
            text = format!("{TAKEN_OVER_NOTE}{text}");
        }
        std::fs::write(local.join(plan::FILE), &text).map_err(|e| internal("write board.toml", e))?;
        self.notify_folders(std::slice::from_ref(&folder));
        tracing::info!(target: "core::arduino", "{name} was taken over: its sketch is code mode now");
        Ok(self.view(name, &folder, &local, &text))
    }

    /// Refuses to compile a guided board's sketch for a board its plan isn't for (the editor's
    /// compile and upload).
    pub(super) fn check_plan_board(
        &self,
        sketch_folder: &Path,
        name: &str,
        board: &str,
    ) -> Result<(), MachineError> {
        let Ok(text) = std::fs::read_to_string(sketch_folder.join(plan::FILE)) else { return Ok(()) };
        let Ok(plan) = read_plan(&text) else { return Ok(()) };
        if plan.taken_over.is_some() || plan.board == board {
            return Ok(());
        }
        // The same pins (a Nano with either bootloader) is the same plan.
        if let (Some(a), Some(b)) = (pins::arduino(&plan.board), pins::arduino(board))
            && a.pins == b.pins
        {
            return Ok(());
        }
        let planned = pins::arduino(&plan.board).map_or(plan.board.clone(), |m| m.name);
        Err(MachineError::new(
            ErrorKind::Conflict,
            "plan-for-another-board",
            format!("{name} is planned for an {planned}: its pins may not exist on this board."),
            format!("Pick {} as the board, or change the board in the plan.", plan.board),
        ))
    }
}

fn taken_over_error(name: &str) -> MachineError {
    MachineError::new(
        ErrorKind::Conflict,
        "plan-taken-over",
        format!("{name}'s code was taken over: Faber no longer writes its sketch."),
        "Edit the sketch in the Arduino editor. To plan the board again, start a new guided board.",
    )
}

fn plan_file_error(name: &str, p: Problem) -> MachineError {
    MachineError::new(ErrorKind::Unprocessable, "plan-file-invalid", format!("{name}: {}", p.message), p.hint)
}

fn invalid_plan(errors: Vec<Problem>) -> MachineError {
    let first = errors.first().map(|p| p.message.clone()).unwrap_or_default();
    let more = errors.len().saturating_sub(1);
    MachineError {
        plan_problems: errors,
        ..MachineError::new(
            ErrorKind::Unprocessable,
            "invalid-plan",
            if more == 0 { first } else { format!("{first} (and {more} more)") },
            "Fix what the problems list says; Faber suggests free pins that fit.",
        )
    }
}

/// The line in `board.toml` a problem is about.
fn locate(text: &str, p: &Problem) -> Option<usize> {
    let part = p.part.as_deref()?;
    let path: Vec<&str> = match (p.role.as_deref(), p.setting.as_deref()) {
        (Some(role), _) => vec!["parts", part, "pins", role],
        (None, Some(setting)) => vec!["parts", part, "settings", setting],
        _ => vec!["parts", part],
    };
    plan::span_of(text, &path).map(|s| plan::line_col(text, s.start).0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arduino::wiring::PlannedPart;

    #[test]
    fn the_pi_header_map_covers_every_header_pin() {
        let (_dir, m) = crate::machine::tests::test_machine();
        let pi = m.pin_map("pi-header").unwrap();
        assert_eq!(pi.pins.len() + pi.power.len(), 40);
        assert_eq!(pi.pins.len(), 28, "GPIO0 to GPIO27");
        assert_eq!(pi.pin("GPIO17").unwrap().physical, Some(11));
        // Drawn as one double header, pin 1 first.
        assert_eq!((pi.headers.len(), pi.headers[0].rows), (1, 2));
        assert_eq!(&pi.headers[0].positions[..4], ["3V3", "5V", "GPIO2", "5V"]);
        assert_eq!(pi.headers[0].positions.len(), 40);
        assert_eq!(m.pin_map("esp32").unwrap_err().code, "unknown-board");
    }

    #[test]
    fn hashes_are_kept_out_of_the_plan() {
        let text =
            "faber-board = 1\nboard = \"uno\"\nname = \"x\"\nsketch-sha256 = \"ab\"\n[parts.a]\ncolour = 1\n";
        assert_eq!(recorded_hash(text).as_deref(), Some("ab"));
        let e = read_plan(text).unwrap_err();
        assert_eq!(e.line, Some(6), "{e:?}");
        assert!(
            read_plan("faber-board = 1\nboard = \"uno\"\nname = \"x\"\nsketch-sha256 = \"ab\"\n").is_ok()
        );
    }

    #[test]
    fn problems_point_at_their_line() {
        let text = "faber-board = 1\nboard = \"uno\"\nname = \"x\"\n\n[parts.front]\ntemplate = \"ultrasonic\"\npins = { trig = 0, echo = 7 }\n";
        let plan = read_plan(text).unwrap();
        let map = pins::arduino("uno").unwrap();
        let problems = allocator::check(&map, &plan.normalized(&map, &BuiltIn), &BuiltIn, &[]);
        let p = problems.iter().find(|p| p.code == "reserved-pin").unwrap();
        assert_eq!(locate(text, p), Some(7));
        let _ = PlannedPart::default();
    }
}
