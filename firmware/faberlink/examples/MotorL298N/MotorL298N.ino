// MotorL298N: two DC motors on an L298N motor driver, driven from Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), an L298N dual H-bridge module, two DC motors,
// a battery or power supply for the motors (the L298N loses about 2 V, so 9-12 V for 6 V motors),
// and a USB cable to the computer running Faber.
//
// Wiring:
//   L298N ENA -> pin 10     (pull the ENA jumper off first: with it on, the motor only does full
//   L298N IN1 -> pin 8       speed, and the Arduino can't set it)
//   L298N IN2 -> pin 7
//   L298N ENB -> pin 5      (pull the ENB jumper off too)
//   L298N IN3 -> pin 4
//   L298N IN4 -> pin 12
//   L298N GND -> GND, and to the motor supply's minus: the grounds must be joined
//   L298N 12V -> the motor supply's plus (never through the Arduino)
//   Motor A to OUT1/OUT2, motor B to OUT3/OUT4
// ENA and ENB set the speed, so they need PWM pins (marked ~); IN1-IN4 can be any pins.
//
// A TB6612FNG works the same way: PWMA/AIN1/AIN2 for ENA/IN1/IN2, PWMB/BIN1/BIN2 for
// ENB/IN3/IN4, and its STBY pin as one more number, e.g. FaberMotor left("left",
// faberlink::DRIVER_EN_IN1_IN2, 10, 8, 7, 11): the library holds STBY high.
//
// What you should see: in Add part, "Check for a FaberLink board" on the board's port finds
// "motor-l298n". Add it (say as "rover"): the motors appear as "rover.left" and "rover.right",
// with the pins each uses (left: EN D10 · IN1 D8 · IN2 D7). Drive them from Parts.
// docs/connect-an-arduino.md walks through it.
//
// If a motor turns the wrong way, swap its two wires on the L298N (or swap IN1 and IN2 here).
// Don't print to Serial in the sketch: the port belongs to FaberLink.
//
// Safety, done by the library: the motors start stopped; each stops by itself 250 ms after
// Faber's heartbeat stops, or 250 ms after its last command; and neither starts again on its own.
// Stopped means EN, IN1 and IN2 low: the L298N lets the motor coast to a stop.

#include <FaberLink.h>

FaberLink link(Serial);

// The L298N takes EN (speed) plus IN1 and IN2 (direction): faberlink::DRIVER_EN_IN1_IN2, then
// the pins in that order.
FaberMotor left("left", faberlink::DRIVER_EN_IN1_IN2, 10, 8, 7);
FaberMotor right("right", faberlink::DRIVER_EN_IN1_IN2, 5, 4, 12);

void setup() {
  // Each motor's speed at full power in rad/s (put in your motors'): with it Faber can send a
  // velocity as well as a power. Leave these out if you don't know it.
  left.setMaxVelocity(15.0);
  right.setMaxVelocity(15.0);
  // The board's name and the sketch's version; Faber shows both.
  link.begin(115200, "motor-l298n", "0.1.0");
}

void loop() {
  link.poll();  // the library does the rest; keep loop() free of delay()
}
