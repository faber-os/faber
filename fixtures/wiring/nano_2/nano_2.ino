// nano_2.ino: Faber writes this file from board.toml. Don't edit it here: change the
// board's plan in Faber, or press Take over the code to make the sketch yours.
// Your own code goes in my_code.h, which Faber never changes.
//
// Board: Arduino Nano (nano)
//
// Wiring:
//   arm: Servo signal (orange or yellow) → D4, Servo V+ (red) → its own 5-6 V supply, Servo GND (brown or black) → the supply's - and the board's GND
//   fan: Module SIG → D5, Module GND → GND, Module V+/V− (or VIN/GND) → the load's own supply, Load → the module's output terminals
//   imu: VCC → 5V, GND → GND, SDA → the board's SDA, SCL → the board's SCL, AD0 → open for 0x68, VCC for 0x69
//   knob: One end → 5V, The other end → GND, Middle (wiper, VRx, VRy, or the divider's middle) → A0
//   wheel: Encoder VCC → 5V, Encoder GND → GND, Encoder A (C1, CLK) → D2, Encoder B (C2, DT) → D7

#include <FaberLink.h>
#include <FaberServo.h>
#include <FaberImu.h>
#include <FaberEncoder.h>

FaberLink link(Serial);

// arm: Servo, Signal D4
FaberServo arm("arm", 4, 0.0, 180.0, faberlink::SERVO_HOLD, 544, 2400);

// fan: MOSFET module (PWM), SIG D5
FaberOutput fan("fan", 5, faberlink::OUTPUT_PWM);

// imu: IMU (MPU6050, GY-521), 
FaberImu imu("imu", faberlink::IMU_ADDRESS_68, 20.0);

// knob: Potentiometer or analog sensor, Signal A0
FaberAnalogIn knob("knob", A0, 10.0, 5.0);

// wheel: Quadrature encoder, A D2, B D7
FaberEncoder wheel("wheel", 2, 7, 0.0, 1.0);

#include "my_code.h"

void setup() {
  link.begin(115200, "nano_2", "guided");
  userSetup();
}

void loop() {
  link.poll();
  userLoop();
}
