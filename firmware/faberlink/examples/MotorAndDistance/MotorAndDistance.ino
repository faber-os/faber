// MotorAndDistance: one DC motor and one analog distance sensor, driven from Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), a DC motor on an H-bridge with a PWM input
// and a direction input, an analog distance sensor (here a Sharp GP2Y0A21 IR sensor), and a USB
// cable to the computer running Faber.
//
// Wiring:
//   H-bridge PWM (speed)  -> pin 9
//   H-bridge DIR          -> pin 8
//   Distance sensor out   -> A0  (and its 5 V and GND)
//
// What you should see: Faber finds the board and suggests adding it. The motor appears as
// "<board>/left" and the sensor as "<board>/front", each with its own viewer, and the sensor's
// reading updates 20 times a second.
//
// If it doesn't work: close the Arduino IDE's Serial Monitor (only one program can use the port
// at a time), and don't print to Serial in the sketch: the port belongs to FaberLink.
//
// Safety, done by the library: the motor starts stopped; it stops by itself 250 ms after Faber's
// heartbeat stops, or 250 ms after its last command; and it never starts again on its own.

#include <FaberLink.h>

FaberLink link(Serial);

// Pin 9 is the speed (PWM), pin 8 the direction. The last number is the motor's speed at full
// duty in rad/s (put in your motor's): with it Faber can send a velocity as well as a duty.
FaberMotor left("left", 9, 8, 15.0);

// The GP2Y0A21's curve: about 29.988 * volts^-1.173 cm (a commonly used fit to its datasheet
// graph, so check it against a ruler). Below 0.4 V nothing is in range.
float sharpToMeters(int raw) {
  float volts = raw * 5.0f / 1023.0f;
  if (volts < 0.4f) return -1;  // no target
  return 0.29988f * pow(volts, -1.173f);
}
FaberDistance front("front", A0, 0.1, 0.8, faberlink::DISTANCE_INFRARED, sharpToMeters);

void setup() {
  // The board's name and the sketch's version; Faber shows both.
  link.begin(115200, "motor-and-distance", "0.1.0");
}

void loop() {
  link.poll();  // the library does the rest; keep loop() free of delay()
}
