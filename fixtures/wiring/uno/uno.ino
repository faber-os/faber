// uno.ino: Faber writes this file from board.toml. Don't edit it here: change the
// board's plan in Faber, or press Take over the code to make the sketch yours.
// Your own code goes in my_code.h, which Faber never changes.
//
// Board: Arduino Uno (uno)
//
// Wiring:
//   battery: Battery + → R1 (20000 Ω) → A1, A1 → R2 (10000 Ω) → GND, Battery - → GND
//   front: VCC → 5V, GND → GND, Trig → D4, Echo → D7
//   front_2: Vcc → 5V, GND → GND, Vo → A0
//   left: Driver PWM → D5, Driver DIR → D8, Driver GND → GND, Driver power → the motor's own supply, Motor → the driver's motor terminals

#include <FaberLink.h>

FaberLink link(Serial);

// battery: Battery voltage (divider), Sense A1
FaberBattery battery("battery", A1, 3.0, 3, faberlink::BATTERY_LI_ION, 5.0, 0.0);

// front: Ultrasonic distance sensor, Trig D4, Echo D7
FaberUltrasonic front("front", 4, 7, 0.02, 4.0);

// front_2: the GP2Y0A21's curve, about 29.988 * volts^-1.173 cm (a commonly used fit to its
// datasheet graph, so check it against a ruler). Below 0.4 V nothing is in range.
float front_2ToMeters(int raw) {
  float volts = raw * 5.0f / 1023.0f;
  if (volts < 0.4f) return -1;  // no target
  return 0.29988f * pow(volts, -1.173f);
}

// front_2: Infrared distance sensor (Sharp GP2Y0A21), Signal A0
FaberDistance front_2("front_2", A0, 0.1, 0.8, faberlink::DISTANCE_INFRARED, front_2ToMeters);

// left: DC motor with a driver, PWM D5, DIR D8
FaberMotor left("left", 5, 8, 0.0);

#include "my_code.h"

void setup() {
  link.begin(115200, "uno", "guided");
  userSetup();
}

void loop() {
  link.poll();
  userLoop();
}
