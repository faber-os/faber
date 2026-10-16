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

  // Adds a custom part (your own faberlink::Part); call it before begin(). FaberMotor,
  // FaberDistance, FaberUltrasonic and FaberBattery add themselves.
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
