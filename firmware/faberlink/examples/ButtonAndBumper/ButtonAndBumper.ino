// ButtonAndBumper: a push button and a bumper (a micro switch), watched by Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), a push button, a micro switch (or a second
// button standing in for the bumper), jumper wires, and a USB cable to the computer running Faber.
// No resistors: each switch goes between its pin and GND, and the pin's own pull-up does the rest.
//
// Wiring:
//   Button: one leg -> pin 4, the other leg -> GND
//   Bumper: COM -> pin 5, NO (normally open) -> GND
//
// What you should see: add the board in Faber (say as "arduino"). "arduino.start" is an on/off
// input (digital-in): press the button and its viewer shows On. "arduino.bumper" is an obstacle
// sensor: press the switch and its viewer shows Detected. Each press is one change, however much
// the switch bounces: the library debounces it (20 ms for the button, 10 ms for the bumper).
//
// The two are different on purpose: an app that asks for a button to start with is offered the
// button, never the bumper.
//
// If it doesn't work: close the Arduino IDE's Serial Monitor (only one program can use the port at
// a time), and don't print to Serial in the sketch: the port belongs to FaberLink.

#include <FaberLink.h>

FaberLink link(Serial);

FaberButton start("start", 4);      // pressed while pin 4 is LOW
FaberObstacle bumper("bumper", 5);  // a bumper: detected while pin 5 is LOW

void setup() {
  link.begin(115200, "buttons", "0.1.0");
}

void loop() {
  link.poll();
  // Your own code can use them too, Faber or not: start.pressed(), bumper.detected().
}
