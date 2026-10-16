// KnobAndJoystick: a potentiometer and a thumb joystick (KY-023), read by Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), a 10 kOhm potentiometer, a thumb joystick
// module, jumper wires, and a USB cable to the computer running Faber.
//
// Wiring:
//   Potentiometer: one end -> 5V, the other end -> GND, the middle (wiper) -> A0
//   Joystick: +5V -> 5V, GND -> GND, VRx -> A1, VRy -> A2, SW -> pin 4
//
// What you should see: add the board in Faber (say as "arduino"). "arduino.knob", "arduino.x" and
// "arduino.y" are analog inputs: their viewers show 0 % to 100 % of full scale and the volts as
// you turn the knob and move the stick (a joystick at rest is near 50 %). "arduino.press" is the
// joystick's push button, an on/off input.

#include <FaberLink.h>

FaberLink link(Serial);

FaberAnalogIn knob("knob", A0);    // 10 readings a second
FaberAnalogIn x("x", A1, 20);      // a joystick moves quickly: 20 a second
FaberAnalogIn y("y", A2, 20);
FaberButton press("press", 4);     // its SW pin closes to GND when pressed

void setup() {
  link.begin(115200, "joystick", "0.1.0");
}

void loop() {
  link.poll();
}
