# The Arduino editor

Write a sketch in Faber, compile it with `arduino-cli` on the machine, and put it on the board
with Faber's own [flasher](connect-an-arduino.md#flash-from-faber). No Arduino IDE and no PC are
needed. Faber also remembers what it put on each board, so it can later open **the code that is on
the board**, but only once it has proved that the board still runs exactly that code.

**Tested or not.** Every step marked *(tested)* runs on every CI run: the core with a stand-in for
`arduino-cli` and simulated bootloaders behind fake serial ports, the FaberLink library's real C++
core for the build id (`core/tests/arduino.rs`, `tools/faberlink-interop`), the window against a
fake core (`shell/src/windows/ArduinoWindow.test.ts`), and the shell in a browser against a core
started with `--fake-arduino-cli` (`shell/e2e/arduino.spec.ts`). The real compile path was tried
once by hand (below). Steps marked *(needs hardware)* have not run on a real board yet.

## What you need

- An Arduino Uno, Nano or Mega 2560 and its USB cable, plugged into the machine running Faber.
- The Arduino tools on the machine: `arduino-cli` and the Arduino AVR core (the compiler for the
  Uno, Nano and Mega). Faber finds an `arduino-cli` that is already there (on the `PATH`, or the
  one inside the Arduino IDE 2), or installs both with one click (below).

## Steps

1. **Open the editor**: Start → Arduino. *(tested)*
2. **Install the tools, once**, if the editor says "The Arduino tools aren't installed". Press
   **Install Arduino tools**. Faber shows what it downloads before it starts: `arduino-cli` from
   Arduino's official releases on GitHub (checked against the SHA-256 Arduino publishes for it;
   nothing is installed if it doesn't match) and the Arduino AVR core, about 150 MB in all. It goes
   into Faber's data folder (`<data>/tools/`), next to nothing else on the machine. The bar shows
   the download, then the core. *(tested with downloads served by the test, never from the
   internet; the real download needs a run on a machine)*
3. **Make a sketch**: **New sketch**, a name (letters, digits, `_`, `-` and `.`, as in the Arduino
   IDE), and what to start from: an empty sketch, a **FaberLink board** (a board Faber talks to,
   see [connect-an-arduino.md](connect-an-arduino.md)), or one of the FaberLink library's examples.
   It is saved in Files as `Arduino/<name>/<name>.ino`, exactly as the Arduino IDE keeps
   sketches. *(tested)*
4. **Write the code.** More files of the sketch (`.h`, `.cpp`) are tabs; **+** adds one. Ctrl+F
   searches, Ctrl+Z undoes. Ctrl+S saves every changed tab. The editor helps while you type
   (below). *(tested)*
5. **Verify** (Ctrl+R) saves and compiles. Errors are listed with their file and line, e.g.
   `motor.h:3:2`; click one to jump there. They are also underlined in the code (red for an
   error, amber for a warning) with a mark in the margin; hover the underline to read it. When it compiles, the size shows like the IDE's:
   "Program: 6 896 bytes of 32 256 (21 %)". The first compile takes a minute (the Arduino core is
   compiled once); later ones are quicker. *(tested)*
6. **Pick the board and the port.** An official Uno or Mega is recognised by its USB ids, and the
   board is picked for you; for a clone with a CH340 or FT232 chip, pick it yourself (an old Nano
   clone is often "Arduino Nano (old bootloader)"). *(tested)*
7. **Upload** (Ctrl+U) compiles, asks once, then puts the program on the board with Faber's
   flasher and reads it back to compare. Keep the board plugged in until it says "Uploaded". If a
   FaberLink board in Parts uses the port, **Stop the board and upload** stops it (its motors stop)
   and it starts again on the new sketch. *(tested on simulated bootloaders; needs hardware)*

FaberLink is always there for a sketch: `#include <FaberLink.h>` works without installing the
library, and it is the copy built into this Faber, so the board always speaks the core's protocol
version (a FaberLink folder in the Arduino IDE's own libraries is not used). Other libraries can't
be installed from the editor yet.

## Help while you type

All of it works offline, from what Faber knows about the Arduino language and from the sketch
itself. *(tested: unit tests for each piece, the window against a fake core, and typing in a
browser in `shell/e2e/arduino.spec.ts`)*

- **Colours.** Keywords, types, your functions, the Arduino API (`pinMode`, `millis`,
  `Serial` …), constants (`HIGH`, `A0`, and ALL_CAPS names, the C++ habit for constants),
  FaberLink's classes (in bold), strings, comments, `#include` and `#define`, and brackets by how
  deeply they nest (three colours in turn). The colours belong to the style you picked in
  Settings, light and dark, and are checked to read well on the editor in every style.
- **Completion.** After two letters of a name the editor offers what fits: the sketch's own
  functions, variables, `#define`s, classes and the local variables in scope (from every tab of
  the sketch), the Arduino API, FaberLink, C++ keywords and snippets (`setup`, `for`, `if`,
  `ifelse`, `switch`, `while`, `every` for a `millis()` timer without `delay()`). After `.` it
  offers what that object has: `Serial.`, `Wire.`, a FaberLink part you declared
  (`FaberUltrasonic front(…); front.`), or your own struct. After `#` the directives, after
  `#include <` the libraries. Ctrl+Space opens the list anywhere; arrows pick, Enter or Tab
  takes it, Escape closes it. Taking a function writes its call with its parameters to type
  over: Tab goes to the next one.
- **Signature help.** Inside a call, the function's parameters show above it, with the one you
  are typing in bold. Escape hides it.
- **Hover docs.** Rest the pointer on a name to see what it is: an Arduino or FaberLink function
  with its signature and what it does (and the `#include` it needs), or your own function,
  variable or class with its declaration and where it is (`motor.h:12`).
- **Errors in the code**, after Verify (above). Editing an underlined line takes its underline
  away, since the message may not be true any more; the next Verify shows the new ones. F8 goes
  to the next one, Ctrl+Shift+M lists them below the code.

The editor reads the sketch the way the C++ grammar sees it, not the way the compiler does: it
can't know the types a library you include defines, or what a macro expands to. A language server
(`clangd`) would know all of it; see "Not built yet".

## What you should see

- After Verify: "Compiled for the Arduino Uno." and the size of the program and of its global
  variables.
- After Upload: "Uploaded: the Arduino Uno runs Blink now."
- For a FaberLink board in Parts: "Code from Faber: Blink" next to its firmware, or "Code not
  flashed from Faber (unknown)" for a sketch uploaded with the Arduino IDE.
- **On the board** (in the editor's toolbar) answers which code runs on the board on the port:

| What it says | When |
|---|---|
| "This board still runs Blink from 16 Oct 2026, 14:02." **Open it** | Proved, by the board's build id or by reading it back. |
| "This board has different code now (changed outside Faber since …). The last code Faber flashed is Blink." **Open it anyway as a copy** | Proved to be other code. |
| "Faber flashed Blink on this board on …. Only reading the program back can show it is still there: that resets the board and takes about 1 s." **Check the code on this board** | Nothing on the port says it live; the check needs your OK. |
| "Faber has never flashed this board." | No record of this board. |

## The same-code check

**Why the code can't simply be read off the chip.** A board holds machine code: the compiler's
output, not the sketch. Names, comments, the layout of the code and which file a line came from
are all gone, and many sketches compile to the same instructions. Reading the flash gives bytes
that can be compared, never source that can be edited. So Faber keeps the source itself.

**What Faber records.** After every successful upload it writes `<data>/arduino/boards.toml`, one
entry per board, and keeps an exact copy of what it flashed in `<data>/arduino/flashed/<id>/`:

- the board's **identity**: its USB serial number when it has one (official Arduinos do), else its
  port id plus USB vendor and product id, marked "can't be told apart from an identical board"
  (CH340 and many FT232 clones have no serial number; Windows makes one up from where the board is
  plugged in, and Faber doesn't use that);
- the board type (`uno`, `mega2560`, …);
- the sketch's path in Files and its name;
- a copy of the sketch's files exactly as they were compiled (Faber compiles a copy, so editing
  the sketch in Files later changes nothing in the record);
- the exact program (Intel HEX);
- a **SHA-256 of the programmed bytes**: every flash page the program was written in, as written
  (unused bytes of a page are `0xFF`, erased flash);
- the sketch's **build id** and the time.

**The two proofs.** A record says what Faber flashed, not what the board runs now: someone may have
uploaded another sketch with the Arduino IDE since. So Faber never says "still runs" without one of
two checks, the strongest available:

1. **A FaberLink board says its build id, live.** Faber compiles every sketch with
   `-DFABER_BUILD_ID=0x…`: the first 8 bytes of a SHA-256 over the sketch's files (each file's
   path, length and bytes) and the board. The FaberLink library sends it at the end of its
   `HELLO_ACK` ([faberlink.md](faberlink.md#3-messages)), so a FaberLink board in Parts says which
   build it runs every time it connects, without being reset. The same id as the record: the same
   code. No id, or another one: other code (a sketch from the Arduino IDE has none). *(tested end
   to end with the library's C++ core)*
2. **Any board: read it back.** **Check the code on this board** resets the board into its
   bootloader, reads back exactly the flash pages of the recorded program, compares them byte for
   byte with the record and their SHA-256 with the recorded one, and leaves the bootloader, so the
   sketch starts again. It writes nothing. Because it resets the board it only runs when you ask
   (a FaberLink board on the port is stopped first, with your OK), and it says how long it takes:
   about 1 s for a 7 KB sketch on an Uno (115 200 baud, 128-byte pages), a few seconds for a
   typical Mega sketch, up to half a minute for a full Mega. *(tested on simulated bootloaders:
   same code, other code, never flashed, a board without a serial number; needs hardware)*

**Opening the code.** When the board is proved to run the record, **Open it** opens the sketch in
Files if it is still exactly what was flashed, else a copy of the flashed files in
`Arduino/<name>-on-board/`. For other code, **Open it anyway as a copy** opens the last thing Faber
flashed, always as a copy, so it is never mistaken for what the board runs.

What the check proves, exactly: the build id names the sketch's source and board; the read-back
proves the program bytes. A sketch compiled from the same source by a newer Faber (with a newer
FaberLink library) has the same build id, because it is the same code to edit.

## If it doesn't work

| What you see | What to try |
|---|---|
| "The Arduino tools aren't installed on this machine yet." | Press **Install Arduino tools**. The machine needs to be on the internet once. |
| "Faber could not download …" | Check that the machine is on the internet (its network cable or Wi-Fi), then try again. |
| "The downloaded … isn't the file Arduino published" | Nothing was installed. Try again; if it keeps happening, the download is being changed on the way. |
| "arduino-cli … doesn't work" | Install the tools again from the editor, or check that program yourself (`arduino-cli version`). |
| "… isn't in an Arduino sketch" | A sketch is a folder with a `.ino` of the same name (`Blink/Blink.ino`). Make a new sketch, or rename the folder. |
| An error in `FaberLink.h` you didn't write | The sketch uses the library in a way it doesn't support; the message says how. |
| "The board didn't answer the bootloader." | As for the flasher: press reset just as you press Upload, and check the board you picked. |
| "… is used by the FaberLink board …" | **Stop the board and upload** (or **Stop the board and check**). |
| A clone board's check says "no serial number of its own" | Faber knows that board only by its port: if you swap two identical clones on the same port, it can't tell. Reading back still proves what is on whichever board is there. |

## How it is built

| Piece | Where |
|---|---|
| Toolchain, records, downloads, library, templates | `core/src/arduino/` (layer 2): `arduino-cli` only behind the `Toolchain` trait; `fake.rs` is the stand-in |
| The machine's part: sketches, compile, upload, the two proofs | `core/src/machine/arduino.rs` |
| The API | `core/src/api/arduino.rs`, [api.md](api.md#the-arduino-editor) |
| Reading a program back | `faber_flash::read_back` (`sdk/rust/flash`) |
| The build id on the wire | `FABER_BUILD_ID` in `firmware/faberlink`, `HelloAck::build_id` in `sdk/rust/faberlink` |
| The window | `shell/src/windows/ArduinoWindow.svelte`, the editor in `shell/src/components/CodeEditor.svelte` and `codemirror.ts` |
| Help while you type | `shell/src/components/editor/`: the catalogue (`arduinoApi.ts`, `faberlinkApi.ts`), the sketch's names from the syntax tree (`symbols.ts`, `sketchIndex.ts`), `completion.ts`, `signature.ts`, `hover.ts`, the extra colours and bracket depth (`semantic.ts`), the compiler's messages in the code (`diagnostics.ts`). All of it is in the editor's chunk, loaded only when an editor opens |
| The colours | `CODE_COLOURS` in `shell/src/lib/desktop/styles.ts` (`--code-*` tokens, the same hues in every style), checked at 4.5:1 on the editor and its active line (7:1 with higher contrast) in `styles.test.ts` |

**Adding an API entry.** The catalogue is plain data in `shell/src/components/editor/`. A
function is one line: `fn('void pinMode(uint8_t pin, uint8_t mode)', 'What it does.')`, with a
list of signatures for overloads; a class has its constructors and members the same way; a
constant, object or type is a name and a description. Write the description in your own words,
short, in English: it documents code, like the code itself, so it is data and not a text of the
interface (it isn't translated; `noRawText.test.ts` skips these two files). `catalogue.test.ts`
checks that every entry has a signature and a description and that no name is there twice.
FaberLink's entries mirror `firmware/faberlink/src/FaberLink.h`: `faberlinkApi.test.ts` reads the
header and fails when a public class or method is added or removed there and not here.

`arduino-cli` is found in this order: Faber's own copy in `<data>/tools/arduino-cli/` (installed
for Faber, with its own data folder under `<data>/tools/`), then one on the `PATH`, then the one the
Arduino IDE 2 brings (`%LOCALAPPDATA%\Programs\Arduino IDE\…` or `Program Files` on Windows, the
app on macOS). One found elsewhere runs with its owner's settings, so the boards installed for the
Arduino IDE are used. Faber runs it with arguments, never through a shell, and reads its
`--format json` answer. Faber installs `arduino-cli` 1.5.1 (`CLI_VERSION` in
`core/src/arduino/install.rs`).

**Tried by hand** (2026-10-16, Windows, the Arduino IDE's `arduino-cli` 1.5.1 and AVR core 1.8.8):
the example `Ultrasonic` compiled for an Uno with `--library firmware/faberlink` and a build id
(6 926 bytes; the library came from that folder, not the IDE's), and a sketch with a broken tab
gave the errors the reader expects (its test uses that output). Every example compiles with and
without a build id, for an Uno and a Mega, with no warnings from the library. Nothing was flashed.

## Not built yet

- Installing other libraries (`arduino-cli lib install`, behind a confirm).
- Boards other than the Uno, Nano and Mega 2560 (the flasher's).
- A serial monitor inside the editor (Tools → Serial monitor works on the port).
- **A language server (`clangd`)**, planned as an option on top of the built-in help: it would
  give completion and errors as you type from the real compiler's view (library types, macros,
  templates), at the cost of a large program on the machine and a compile database per sketch.
  The built-in help stays, so the editor works without it.
