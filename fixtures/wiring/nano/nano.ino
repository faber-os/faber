// nano.ino: Faber writes this file from board.toml. Don't edit it here: change the
// board's plan in Faber, or press Take over the code to make the sketch yours.
// Your own code goes in my_code.h, which Faber never changes.
//
// Board: Arduino Nano (nano)
//
// Wiring:
//   battery: Battery + → R1 (20000 Ω) → A1, A1 → R2 (10000 Ω) → GND, Battery - → GND
//   bumper: Switch COM → A3, Switch NO (normally open) → GND
//   front: VCC → 5V, GND → GND, Trig → D4, Echo → D7
//   front_2: Vcc → 5V, GND → GND, Vo → A0
//   front_3: VCC → 5V, GND → GND, OUT → A4
//   left: Driver PWM → D5, Driver DIR → D8, Driver GND → GND, Driver power → the motor's own supply, Motor → the driver's motor terminals
//   left_2: Driver EN (ENA, PWMA) → D6 (on an L298N, take the jumper off ENA), Driver IN1 → D12, Driver IN2 → D9, Driver GND → GND, Driver power → the motor's own supply, Motor → the driver's motor terminals
//   left_3: Driver IN1 (IA, AIN1, RPWM) → D11, Driver IN2 (IB, AIN2, LPWM) → D10, Driver GND → GND, Driver power → the motor's own supply, Motor → the driver's motor terminals
//   start: One leg → A2, The other leg → GND (or, for a TTP223: VCC → 5V, GND → GND, I/O → A2)

#include <FaberLink.h>

FaberLink link(Serial);

// battery: Battery voltage (divider), Sense A1
FaberBattery battery("battery", A1, 3.0, 3, faberlink::BATTERY_LI_ION, 5.0, 0.0);

// bumper: Bumper (micro switch), Switch A3
FaberObstacle bumper("bumper", A3, faberlink::OBSTACLE_BUMPER);

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

// front_3: Infrared obstacle sensor (FC-51, E18-D80NK), OUT A4
FaberObstacle front_3("front_3", A4, faberlink::OBSTACLE_INFRARED, faberlink::ACTIVE_LOW, 0.0);

// left: DC motor with a driver, PWM D5, DIR D8
FaberMotor left("left", 5, 8, 0.0);

// left_2: DC motor with a driver, EN D6, IN1 D12, IN2 D9
FaberMotor left_2("left_2", faberlink::DRIVER_EN_IN1_IN2, 6, 12, 9);

// left_3: DC motor with a driver, IN1 D11, IN2 D10
FaberMotor left_3("left_3", faberlink::DRIVER_TWO_PWM, 11, 10);

// start: Button or switch, Signal A2
FaberButton start("start", A2, faberlink::ACTIVE_LOW, 20);

#include "my_code.h"

void setup() {
  left_2.setMaxVelocity(0);
  left_3.setMaxVelocity(0);
  link.begin(115200, "nano", "guided");
  userSetup();
}

void loop() {
  link.poll();
  userLoop();
}
