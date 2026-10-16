// FaberLink for Arduino: connect a board to Faber.
//
//   #include <FaberLink.h>
//
//   FaberLink link(Serial);
//   FaberMotor left("left", 9, 8);        // PWM pin, direction pin
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

// A DC motor on an H-bridge: a PWM pin for the speed and a pin for the direction. Give
// maxVelocityRadS (its speed at full duty, in rad/s) to accept set-velocity too; without it the
// motor takes set-duty only.
class FaberMotor : public faberlink::Motor {
 public:
  FaberMotor(const char* name, uint8_t pwmPin, uint8_t directionPin, float maxVelocityRadS = 0);
  void begin() override;

 protected:
  void drive(float duty) override;

 private:
  uint8_t pwmPin_, directionPin_;
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
