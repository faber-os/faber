// nano-old_2.ino: Faber writes this file from board.toml. Don't edit it here: change the
// board's plan in Faber, or press Take over the code to make the sketch yours.
// Your own code goes in my_code.h, which Faber never changes.
//
// Board: Arduino Nano (old bootloader) (nano-old)
//
// Wiring:
//   fan: Module SIG → D5, Module GND → GND, Module V+/V− (or VIN/GND) → the load's own supply, Load → the module's output terminals

#include <FaberLink.h>

FaberLink link(Serial);

// fan: MOSFET module (PWM), SIG D5
FaberOutput fan("fan", 5, faberlink::OUTPUT_PWM);

#include "my_code.h"

void setup() {
  link.begin(115200, "nano-old_2", "guided");
  userSetup();
}

void loop() {
  link.poll();
  userLoop();
}
