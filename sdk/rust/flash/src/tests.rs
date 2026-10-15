//! The flasher against the simulated bootloaders: a fake flash memory, written, read back and
//! compared, and every way it can go wrong.

use crate::board::{self, MEGA2560, NANO_OLD, Protocol, UNO};
use crate::sim::{Faults, SimBoard, SimPort};
use crate::{FlashError, Flashed, Image, Progress, Stage, Timing, flash};

/// A program of `len` bytes that isn't all one value, so a page in the wrong place shows.
fn program(len: usize) -> Image {
    Image::from_bytes(&(0..len).map(|i| (i * 31 + i / 256) as u8).collect::<Vec<_>>())
}

fn run(
    board: &board::Board,
    sim: SimBoard,
    image: &Image,
) -> (Result<Flashed, FlashError>, SimPort, Vec<Progress>) {
    let mut port = SimPort::new(sim);
    let mut seen = Vec::new();
    let r = flash(&mut port, board, image, &Timing::default(), &mut |p| seen.push(p));
    (r, port, seen)
}

fn assert_flashed(port: &SimPort, image: &Image) {
    assert_eq!(&port.board.flash()[..image.end() as usize], image.bytes());
    assert!(port.board.started_new_sketch(), "the bootloader starts the new program");
}

#[test]
fn an_uno_takes_the_program_and_reads_it_back() {
    let image = program(5000);
    let (r, port, seen) = run(&UNO, SimBoard::new(&UNO), &image);
    assert_eq!(r, Ok(Flashed { bytes: 5000, pages: 40 }));
    assert_flashed(&port, &image);
    assert_eq!(port.board.resets(), 1, "one reset pulse");
    assert_eq!(seen.first().map(|p| p.stage), Some(Stage::Connecting));
    assert_eq!(seen.last(), Some(&Progress { stage: Stage::Done, done: 5120, total: 5120 }));
    let writes = seen.iter().filter(|p| p.stage == Stage::Writing).count();
    let checks = seen.iter().filter(|p| p.stage == Stage::Verifying).count();
    assert_eq!((writes, checks), (40, 40), "progress after every page");
}

#[test]
fn a_mega_takes_the_program_across_the_64k_word_boundary() {
    // 140 KB reaches past 128 KB, where the address needs its extended byte.
    let image = program(140_000);
    let (r, port, _) = run(&MEGA2560, SimBoard::new(&MEGA2560), &image);
    assert_eq!(r.map(|f| f.pages), Ok(547));
    assert_flashed(&port, &image);
}

#[test]
fn a_real_blink_hex_goes_on_an_old_nano() {
    let hex = ":100000000C945C000C946E000C946E000C946E00CA\n\
               :100010000C946E000C946E000C946E000C946E00A8\n\
               :00000001FF\n";
    let image = Image::parse(hex).unwrap();
    let (r, port, _) = run(&NANO_OLD, SimBoard::new(&NANO_OLD), &image);
    assert_eq!(r, Ok(Flashed { bytes: 32, pages: 1 }));
    assert_flashed(&port, &image);
}

#[test]
fn gaps_in_the_file_are_left_alone() {
    let image = Image::parse(":020000040000FA\n:020100001122CA\n:00000001FF\n").unwrap();
    let (r, port, _) = run(&UNO, SimBoard::new(&UNO), &image);
    assert_eq!(r.map(|f| f.pages), Ok(1), "only the page with data is written");
    assert_eq!(port.board.pages_written(), 1);
    assert_eq!(port.board.flash()[0x100..0x102], [0x11, 0x22]);
}

#[test]
fn a_program_too_big_is_refused_before_the_port_is_touched() {
    let (r, port, seen) = run(&UNO, SimBoard::new(&UNO), &program(32_257));
    assert_eq!(r, Err(FlashError::TooLarge { board: "Arduino Uno", end: 32_257, room: 32_256 }));
    assert!(port.sent.is_empty() && port.board.resets() == 0 && seen.is_empty());
    let fits = run(&UNO, SimBoard::new(&UNO), &program(32_256)).0;
    assert_eq!(fits.map(|f| f.pages), Ok(252), "exactly full fits");
}

#[test]
fn no_bootloader_means_no_sync_and_nothing_written() {
    for board in [&UNO, &MEGA2560] {
        let sim = SimBoard::new(board).with_faults(Faults { no_bootloader: true, ..Faults::default() });
        let (r, port, _) = run(board, sim, &program(300));
        assert_eq!(r, Err(FlashError::NoSync { board: board.name }), "{}", board.id);
        assert_eq!(port.board.pages_written(), 0);
        let e = r.unwrap_err();
        assert_eq!(e.message(), "The board didn't answer the bootloader.");
        assert!(e.hint().contains("pressing reset just as you press Flash"), "{}", e.hint());
    }
}

#[test]
fn the_wrong_protocol_finds_no_bootloader() {
    // An Uno picked as a Mega (or the other way round): the bootloaders don't understand each
    // other, so nothing answers and nothing is written.
    let (r, port, _) = run(&MEGA2560, SimBoard::new(&UNO), &program(300));
    assert_eq!(r, Err(FlashError::NoSync { board: "Arduino Mega 2560" }));
    assert_eq!(port.board.pages_written(), 0);
    let (r, port, _) = run(&UNO, SimBoard::new(&MEGA2560), &program(300));
    assert_eq!(r, Err(FlashError::NoSync { board: "Arduino Uno" }));
    assert_eq!(port.board.pages_written(), 0);
}

#[test]
fn the_wrong_chip_is_refused_before_writing() {
    // An old Nano with an ATmega168, picked as an Uno: same bootloader language, other chip.
    let sim = SimBoard::with_chip(Protocol::Stk500v1, [0x1E, 0x94, 0x06], 16_384);
    let (r, port, _) = run(&UNO, sim, &program(300));
    let e = r.unwrap_err();
    assert_eq!(
        e,
        FlashError::WrongSignature {
            board: "Arduino Uno",
            expected: UNO.signature,
            found: [0x1E, 0x94, 0x06]
        }
    );
    assert_eq!(e.code(), "wrong-board");
    assert_eq!(
        e.message(),
        "This board has an ATmega168 (1E 94 06), but an Arduino Uno has an ATmega328P (1E 95 0F)."
    );
    assert_eq!(port.board.pages_written(), 0);
    // A Mega 1280 picked as a Mega 2560.
    let sim = SimBoard::with_chip(Protocol::Stk500v2, [0x1E, 0x97, 0x03], 131_072);
    let (r, port, _) = run(&MEGA2560, sim, &program(300));
    assert!(matches!(r, Err(FlashError::WrongSignature { found: [0x1E, 0x97, 0x03], .. })), "{r:?}");
    assert_eq!(port.board.pages_written(), 0);
}

#[test]
fn a_bad_flash_cell_fails_the_verify_with_its_address() {
    for board in [&UNO, &MEGA2560] {
        // Byte 0x103 of the program is even, so the stuck bit shows.
        let image = program(1000);
        assert_eq!(image.bytes()[0x103] & 1, 0);
        let sim = SimBoard::new(board).with_faults(Faults { bad_cell: Some(0x103), ..Faults::default() });
        let (r, port, _) = run(board, sim, &image);
        let wrote = image.bytes()[0x103];
        assert_eq!(
            r,
            Err(FlashError::VerifyFailed { address: 0x103, wrote, read: wrote | 1 }),
            "{}",
            board.id
        );
        assert!(port.board.started_new_sketch(), "the bootloader is left even so");
    }
}

#[test]
fn a_board_that_goes_silent_mid_write_says_so() {
    for board in [&UNO, &MEGA2560] {
        let sim =
            SimBoard::new(board).with_faults(Faults { silent_after_pages: Some(3), ..Faults::default() });
        let (r, port, seen) = run(board, sim, &program(4000));
        assert_eq!(r, Err(FlashError::LostSync { while_: "writing the program" }), "{}", board.id);
        assert_eq!(port.board.pages_written(), 3);
        assert_eq!(seen.iter().filter(|p| p.stage == Stage::Writing).count(), 3);
        assert_eq!(r.unwrap_err().code(), "board-stopped-answering");
    }
}

#[test]
fn a_few_damaged_answers_are_sent_again() {
    let sim = SimBoard::new(&MEGA2560).with_faults(Faults { damaged_answers: 3, ..Faults::default() });
    let image = program(2000);
    let (r, port, _) = run(&MEGA2560, sim, &image);
    assert!(r.is_ok(), "{r:?}");
    assert_flashed(&port, &image);
}

#[test]
fn answers_that_stay_damaged_give_up() {
    let sim = SimBoard::new(&MEGA2560).with_faults(Faults { damaged_answers: 1000, ..Faults::default() });
    let (r, port, _) = run(&MEGA2560, sim, &program(2000));
    // The sign-on itself never arrives intact.
    assert_eq!(r, Err(FlashError::NoSync { board: "Arduino Mega 2560" }));
    assert_eq!(port.board.pages_written(), 0);
}

#[test]
fn damage_after_the_sign_on_is_reported_as_damage() {
    let faults = Faults { damaged_answers: 1000, intact_answers: 1, ..Faults::default() };
    let (r, port, _) = run(&MEGA2560, SimBoard::new(&MEGA2560).with_faults(faults), &program(500));
    let e = r.unwrap_err();
    assert_eq!(e, FlashError::DamagedAnswers);
    assert!(e.hint().contains("USB cable"), "{}", e.hint());
    assert_eq!(port.board.pages_written(), 0);
}

#[test]
fn optiboot_answers_byte_for_byte_as_avr061_says() {
    let mut port = SimPort::new(SimBoard::new(&UNO));
    let r = flash(&mut port, &UNO, &program(10), &Timing::default(), &mut |_| {});
    assert!(r.is_ok());
    // The first command after the reset is GET_SYNC, then ENTER_PROGMODE and READ_SIGN.
    assert_eq!(&port.sent[..6], &[0x30, 0x20, 0x50, 0x20, 0x75, 0x20]);
    // Then LOAD_ADDRESS 0 and PROG_PAGE of 128 flash bytes, ending in CRC_EOP.
    assert_eq!(&port.sent[6..14], &[0x55, 0x00, 0x00, 0x20, 0x64, 0x00, 0x80, b'F']);
    assert_eq!(port.sent[14 + 128], 0x20);
    // And it ends with LEAVE_PROGMODE.
    assert_eq!(&port.sent[port.sent.len() - 2..], &[0x51, 0x20]);
}
