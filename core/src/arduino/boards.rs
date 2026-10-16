//! Which `arduino-cli` board (its FQBN) each of the flasher's boards is.
//!
//! The FQBNs are the Arduino AVR core's own (`boards.txt`): the Nano's two bootloaders are a menu
//! option of the one board, and the Mega 2560 is the Mega board with its 2560 chip.

/// The fully qualified board name `arduino-cli` compiles for, by the flasher's board id
/// ([`faber_flash::Board::id`]).
pub fn fqbn(board: &str) -> Option<&'static str> {
    Some(match board {
        "uno" => "arduino:avr:uno",
        "nano" => "arduino:avr:nano:cpu=atmega328",
        "nano-old" => "arduino:avr:nano:cpu=atmega328old",
        "mega2560" => "arduino:avr:mega:cpu=atmega2560",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_board_the_flasher_knows_can_be_compiled_for() {
        for b in faber_flash::BOARDS {
            assert!(fqbn(b.id).is_some_and(|f| f.starts_with("arduino:avr:")), "{}", b.id);
        }
        assert_eq!(fqbn("esp32"), None);
    }
}
