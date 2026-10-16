// ServoArm: one hobby servo (an SG90 or MG90S) that Faber turns to an angle.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), a micro servo, a 5-6 V supply for the servo
// (4 AA cells, or a 5 V power bank with a cut cable), jumper wires, and a USB cable to the
// computer running Faber. The Servo library comes with the Arduino IDE.
//
// Wiring (servo wires: brown or black GND, red V+, orange or yellow signal):
//   Servo signal -> pin 9
//   Servo V+     -> the servo supply's +
//   Servo GND    -> the servo supply's - AND the Arduino's GND (join the grounds)
// A single SG90 resting may run from the Arduino's 5V pin, but moving it can reset the board:
// give it its own supply.
//
// What you should see: add the board in Faber (say as "arduino"). "arduino.arm" is a servo, and
// its viewer shows "Released" until Faber sends the first angle: it doesn't jump at power-up.
// Send it an angle (Faber speaks radians: 1.57 is 90 degrees) and it turns there. When nobody has
// commanded it for a second it holds its angle; declare it with faberlink::SERVO_RELEASE to make
// it go limp instead.
//
// With a servo in the sketch, analogWrite() no longer works on pins 9 and 10 of an Uno (pins 44,
// 45 and 46 of a Mega): the Servo library uses their timer.

#include <FaberLink.h>
#include <FaberServo.h>

FaberLink link(Serial);

FaberServo arm("arm", 9);  // 0 to 180 degrees, holds its angle when stopped

void setup() {
  link.begin(115200, "servo", "0.1.0");
}

void loop() {
  link.poll();
}
