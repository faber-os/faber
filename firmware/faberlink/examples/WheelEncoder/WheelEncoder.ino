// WheelEncoder: a gear motor with a hall encoder (such as a JGB37-520), measured by Faber.
//
// What you need: an Arduino Uno or Nano (a Mega works too), a gear motor with a two-channel
// encoder, a motor driver with PWM and DIR inputs (a Cytron MD10C; for an L298N see the
// MotorL298N example), the motor's own supply, jumper wires, and a USB cable to the computer
// running Faber.
//
// Wiring:
//   Encoder VCC -> 5V, encoder GND -> GND
//   Encoder A   -> pin 2 (an interrupt pin)
//   Encoder B   -> pin 3 (the other interrupt pin: every edge counts)
//   Driver PWM  -> pin 9, driver DIR -> pin 8, driver GND -> GND
//   Driver power -> the motor's own supply; never the board's 5V
//
// Set PULSES_PER_REV and GEAR_RATIO from the motor's data: the encoder's pulses for one turn of
// the motor, and its gearbox. With both, Faber shows the wheel's speed in rad/s.
//
// What you should see: add the board in Faber (say as "arduino"). "arduino.wheel" is an encoder:
// turn the wheel by hand and its count goes up one way and down the other. "arduino.left" is the
// motor: drive it, and its viewer shows the speed the encoder measures, not just the duty it was
// given. If the count goes down while the motor drives forward, swap A and B.

#include <FaberLink.h>
#include <FaberEncoder.h>

const float PULSES_PER_REV = 11;  // a common hall encoder; check your motor's data
const float GEAR_RATIO = 34.02f;  // e.g. a 1:34 gearbox

FaberLink link(Serial);

FaberEncoder wheel("wheel", 2, 3, PULSES_PER_REV, GEAR_RATIO);
FaberMotor left("left", 9, 8);

void setup() {
  left.useEncoder(wheel);
  link.begin(115200, "wheel", "0.1.0");
}

void loop() {
  link.poll();
}
