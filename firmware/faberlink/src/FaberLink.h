// FaberLink for Arduino: connect a board to Faber.
//
//   #include <FaberLink.h>
//
//   FaberLink link(Serial);
//   FaberMotor left("left", 9, 8);        // PWM pin, direction pin (other drivers: see FaberMotor)
//   FaberDistance front("front", A0);     // an analog distance sensor
//
//   void setup() { link.begin(115200, "my-robot", "0.1.0"); }
//   void loop()  { link.poll(); }
//
// Everything else, including stopping the motors when Faber goes quiet, is done by the library.
// This file is the thin Arduino glue; the protocol itself is in FaberLinkCore.h.

#ifndef FABERLINK_H
#define FABERLINK_H

#include <Arduino.h>

#include "FaberLinkCore.h"

namespace faberlink {

// How a DC motor's driver (H-bridge) takes its speed and direction: FaberMotor's driver shapes.
enum MotorDriver : uint8_t {
  // A PWM input for the speed and a direction input: Cytron MD10C, MDD10A.
  DRIVER_PWM_DIR = 0,
  // EN (PWM, the speed) plus IN1 and IN2 (the direction): L298N (jumper off ENA), L293D, TB6612FNG.
  DRIVER_EN_IN1_IN2 = 1,
  // Two PWM inputs, one per direction: L9110S / HG7881, DRV8833, BTS7960 (IBT-2: RPWM, LPWM).
  DRIVER_TWO_PWM = 2,
};

// What a FaberOutput does with its pin.
enum OutputMode : uint8_t {
  // On or off (digital-out@1): an LED, a relay module, an active buzzer. The default.
  OUTPUT_ON_OFF = 0,
  // A duty 0..1 with analogWrite() (pwm-out@1): a dimmed LED, a MOSFET driving a fan or a lamp.
  // The pin must be a PWM pin (marked ~ on the board).
  OUTPUT_PWM = 1,
};

// Which level of a pin means "active": pressed or detected for an input, on for an output.
enum ActiveLevel : uint8_t {
  // LOW is active. An input gets its internal pull-up: a button or a switch between the pin and
  // GND, or a module that pulls its output low (FC-51, E18-D80NK); the inputs' default. An output
  // is on while LOW: most relay modules.
  ACTIVE_LOW = 0,
  // HIGH is active. An input gets no pull-up: a module that drives its output high (a TTP223
  // touch pad); a plain switch to 5V needs a pull-down resistor (10 kOhm) from the pin to GND. An
  // output is on while HIGH: an LED from the pin to GND; the outputs' default.
  ACTIVE_HIGH = 1,
};

}  // namespace faberlink

// A DC motor on a motor driver (H-bridge). The pins depend on the driver:
//
//   FaberMotor left("left", 9, 8);                                       // PWM 9, DIR 8
//   FaberMotor left("left", faberlink::DRIVER_EN_IN1_IN2, 10, 8, 7);     // EN 10, IN1 8, IN2 7
//   FaberMotor left("left", faberlink::DRIVER_EN_IN1_IN2, 10, 8, 7, 4);  // ... and STBY 4 (TB6612)
//   FaberMotor left("left", faberlink::DRIVER_TWO_PWM, 5, 6);            // IN1 5, IN2 6, both PWM
//   FaberMotor left("left", faberlink::DRIVER_TWO_PWM, 5, 6, 4);         // ... and an enable on 4
//   FaberMotor left("left", faberlink::DRIVER_PWM_DIR, 9, 8);            // the same as the first
//
// The speed pins (PWM, EN, and both inputs of DRIVER_TWO_PWM) must be PWM pins (marked ~ on the
// board). A standby or enable pin (TB6612 STBY, DRV8833 nSLEEP, a BTS7960's R_EN and L_EN joined)
// is set HIGH at begin() and stays HIGH: two motors often share it, so one motor stopping must
// not stop the other. A motor missing a pin its driver needs refuses every command.
//
// Stopped (the safe state, and duty 0) means every speed and direction pin LOW. On an L298N
// (EN low) and a DRV8833 or L9110 (both inputs low) the motor coasts: it is let go and rolls to a
// stop. On a TB6612, IN1 = IN2 = LOW is its stop mode (outputs off). A BTS7960 with its enables
// HIGH brakes instead (both half-bridges pull the motor's wires to ground), and a Cytron MD10C
// brakes with PWM LOW: those drivers decide it themselves.
//
// Give maxVelocityRadS, or call setMaxVelocity() (its speed at full duty, in rad/s), to accept
// set-velocity too; without it the motor takes set-duty only.
class FaberMotor : public faberlink::Motor {
 public:
  FaberMotor(const char* name, uint8_t pwmPin, uint8_t directionPin, float maxVelocityRadS = 0);
  FaberMotor(const char* name, faberlink::MotorDriver driver, uint8_t pin1, uint8_t pin2,
             uint8_t pin3 = faberlink::NO_PIN, uint8_t pin4 = faberlink::NO_PIN);
  void begin() override;

 protected:
  uint8_t writePins(uint8_t* out) const override;
  uint8_t apply(const uint8_t* command, uint8_t len, uint8_t* out, bool* clamped) override;
  void drive(float duty) override;

 private:
  bool wired() const;
  // PWM_DIR: pwm, dir. EN_IN1_IN2: en, in1, in2. TWO_PWM: in1, in2 (c_ unused).
  uint8_t a_, b_, c_, standby_;
  faberlink::MotorDriver driver_;
};

// An analog distance sensor. By default the reading is linear: 0 V is 0 m and the reference
// voltage is maxM. Sensors with a curve (e.g. Sharp IR) pass a function from the raw analogRead()
// value to metres; it returns a negative number for "no target".
class FaberDistance : public faberlink::Distance {
 public:
  typedef float (*ToMeters)(int raw);
  FaberDistance(const char* name, uint8_t analogPin, float minM = 0.1f, float maxM = 0.8f,
                faberlink::DistanceKind kind = faberlink::DISTANCE_INFRARED, ToMeters toMeters = 0);

 protected:
  uint8_t writePins(uint8_t* out) const override;
  float measure() override;

 private:
  ToMeters toMeters_;
  float maxM_;
  uint8_t pin_;
};

// An ultrasonic range finder with a trigger pin and an echo pin, such as the HC-SR04
// (2 cm to 4 m, 15 degree cone, at most one measurement per 60 ms: its datasheet's numbers).
// A measurement waits for the echo, at most about 24 ms for maxM = 4 m, so loop() stays fast
// enough for FaberLink. Echoes take the speed of sound at about 20 degrees C (343 m/s).
class FaberUltrasonic : public faberlink::Distance {
 public:
  FaberUltrasonic(const char* name, uint8_t triggerPin, uint8_t echoPin, float minM = 0.02f,
                  float maxM = 4.0f);
  void begin() override;

 protected:
  uint8_t writePins(uint8_t* out) const override;
  float measure() override;

 private:
  unsigned long timeoutUs_;
  uint8_t triggerPin_, echoPin_;
};

// A button, switch or end stop on a digital pin (digital-in@1), debounced by the library. Wired
// between the pin and GND it needs nothing else: the pin's pull-up holds it HIGH, pressed is LOW.
//
//   FaberButton start("start", 4);                                   // a button to GND
//   FaberButton touch("touch", 5, faberlink::ACTIVE_HIGH);           // a TTP223 touch pad
//
// pressed() is the debounced state, for your own code too; Faber sees every change.
class FaberButton : public faberlink::DigitalIn {
 public:
  FaberButton(const char* name, uint8_t pin, faberlink::ActiveLevel active = faberlink::ACTIVE_LOW,
              uint16_t debounceMs = 20);
  void begin() override;
  bool pressed() const { return level(); }

 protected:
  uint8_t writePins(uint8_t* out) const override;
  bool readLevel() override;

 private:
  uint8_t pin_;
  faberlink::ActiveLevel active_;
};

// A bumper, whisker, limit switch or digital infrared obstacle module (obstacle@1): it only says
// whether something is in the way. Debounced like a button.
//
//   FaberObstacle bumper("bumper", 4);                                          // a micro switch to GND
//   FaberObstacle ir("ir", 5, faberlink::OBSTACLE_INFRARED, faberlink::ACTIVE_LOW, 0.1f);  // FC-51
//
// triggerDistanceM: how close something must come (an infrared module's potentiometer sets it);
// 0 if you don't know. detected() is the debounced state, for your own code too.
class FaberObstacle : public faberlink::Obstacle {
 public:
  FaberObstacle(const char* name, uint8_t pin, faberlink::ObstacleKind kind = faberlink::OBSTACLE_BUMPER,
                faberlink::ActiveLevel active = faberlink::ACTIVE_LOW, float triggerDistanceM = 0);
  void begin() override;
  bool detected() const { return level(); }

 protected:
  uint8_t writePins(uint8_t* out) const override;
  bool readLevel() override;

 private:
  uint8_t pin_;
  faberlink::ActiveLevel active_;
};

// An output on one pin that Faber switches: on/off (digital-out@1) or a PWM duty (pwm-out@1).
//
//   FaberOutput led("led", 13);                                                   // on or off
//   FaberOutput lamp("lamp", 5, faberlink::OUTPUT_PWM);                           // dimmable, a ~ pin
//   FaberOutput relay("relay", 7, faberlink::OUTPUT_ON_OFF, faberlink::ACTIVE_LOW); // a relay module
//
// Faber leases it like a motor: when nobody has sent a command for 1 s, on the e-stop and when
// Faber goes quiet, it goes off. "Off" is the pin's inactive level: LOW, or HIGH for ACTIVE_LOW,
// which it also is from begin() on. A pin can give about 20 mA: an LED needs its resistor
// (220 Ohm to 1 kOhm), and anything bigger (a relay coil, a motor, a lamp) a module or a
// transistor between it and the pin.
class FaberOutput : public faberlink::PinOutput {
 public:
  FaberOutput(const char* name, uint8_t pin, faberlink::OutputMode mode = faberlink::OUTPUT_ON_OFF,
              faberlink::ActiveLevel active = faberlink::ACTIVE_HIGH);
  void begin() override;

 protected:
  uint8_t writePins(uint8_t* out) const override;
  void drive(float value) override;

 private:
  uint8_t pin_;
  faberlink::ActiveLevel active_;
};

// A battery measured through a voltage divider on an analog pin: battery + -> R1 -> pin -> R2 -> GND.
// dividerRatio is (R1 + R2) / R2, e.g. 3.0 for 20 kOhm over 10 kOhm, which brings a 3S pack's
// 12.6 V down to 4.2 V, safely under a 5 V board's reference. Pick resistors that keep the pin
// below the reference voltage when the battery is full, and measure the ratio with a multimeter if
// you can (resistors are only within 1-5 %). referenceVolts is the board's analog reference: 5.0
// on an Uno or Mega, 3.3 on 3.3 V boards.
class FaberBattery : public faberlink::Battery {
 public:
  FaberBattery(const char* name, uint8_t analogPin, float dividerRatio, uint8_t cellsInSeries,
               faberlink::BatteryChemistry chemistry = faberlink::BATTERY_LI_ION,
               float referenceVolts = 5.0f, float capacityAh = 0);

 protected:
  uint8_t writePins(uint8_t* out) const override;
  float measureVolts() override;

 private:
  float voltsPerCount_;
  uint8_t pin_;
};

class FaberLink {
 public:
  // A hardware serial port: begin() also starts it at the given baud rate.
  explicit FaberLink(HardwareSerial& port);
  // Any other stream (e.g. the USB port on a Leonardo): start it yourself before begin().
  explicit FaberLink(Stream& port);

  // heartbeatTimeoutMs: no heartbeat from Faber for this long and every motor stops (100-1000).
  void begin(unsigned long baud, const char* board, const char* firmwareVersion,
             uint16_t heartbeatTimeoutMs = faberlink::DEFAULT_HEARTBEAT_TIMEOUT_MS);
  // Call it in every loop(), and keep loop() fast: delay() stops the protocol too.
  void poll();

  // Adds a custom part (your own faberlink::Part); call it before begin(). The library's own
  // parts (FaberMotor, FaberButton, ...) add themselves.
  bool add(faberlink::Part& part);
  // A hardware stop button: engageEstop() stops everything and latches; the motors may only run
  // again after resetEstop() here AND a release in Faber.
  void engageEstop();
  void resetEstop();
  void log(const char* text);
  bool connected() const { return board_.connected(); }

 private:
  class StreamOutput : public faberlink::Output {
   public:
    explicit StreamOutput(Stream& s) : stream_(s) {}
    void write(const uint8_t* data, size_t len) override { stream_.write(data, len); }

   private:
    Stream& stream_;
  };
  Stream& port_;
  HardwareSerial* serial_;
  StreamOutput output_;
  faberlink::Board board_;
};

#endif
