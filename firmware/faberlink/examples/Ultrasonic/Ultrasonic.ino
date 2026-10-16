// Ultrasonic: one HC-SR04 ultrasonic range finder, measured by Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), an HC-SR04, four jumper wires, and a USB
// cable to the computer running Faber. Nothing moves in this sketch.
//
// Wiring (the HC-SR04 runs on 5 V, like an Uno or a Mega):
//   HC-SR04 VCC  -> 5V
//   HC-SR04 Trig -> pin 7
//   HC-SR04 Echo -> pin 6
//   HC-SR04 GND  -> GND
//
// What you should see: in Add part, "Check for a FaberLink board" on the board's port finds
// "ultrasonic". Add it (say as "arduino"): the sensor appears as "arduino.front". Its viewer shows
// the distance about 16 times a second; hold your hand in front of it and move it. Nothing in front
// (or further than 4 m) shows as "no target".
//
// If it doesn't work: close the Arduino IDE's Serial Monitor (only one program can use the port at
// a time), and don't print to Serial in the sketch: the port belongs to FaberLink. A reading stuck
// at "no target" usually means Trig and Echo are swapped.

#include <FaberLink.h>

FaberLink link(Serial);

// Trigger on pin 7, echo on pin 6, 2 cm to 4 m (the HC-SR04's datasheet range).
FaberUltrasonic front("front", 7, 6);

void setup() {
  // The board's name and the sketch's version; Faber shows both.
  link.begin(115200, "ultrasonic", "0.1.0");
}

void loop() {
  link.poll();  // the library does the rest; keep loop() free of delay()
}
