// FaberServo: a hobby servo (SG90, MG90S, MG996R) that Faber turns to an angle, on the Arduino
// Servo library. An optional header: include it after FaberLink.h only in sketches with a servo,
// so every other sketch builds without the Servo library.
//
//   #include <FaberLink.h>
//   #include <FaberServo.h>
//
//   FaberServo arm("arm", 9);                                   // 0 to 180 degrees
//   FaberServo pan("pan", 10, 30, 150, faberlink::SERVO_RELEASE);  // 30 to 150, limp when stopped
//
// Any digital pin works: the Servo library makes the pulses with a timer, not the pin's PWM. That
// timer is why, on an Uno, analogWrite() stops working on pins 9 and 10 as soon as a sketch has
// a servo (on a Mega, pins 44, 45 and 46).
//
// Angles in the sketch are degrees, as the Servo library has them; Faber gets radians. minDeg and
// maxDeg are the angles minPulseUs and maxPulseUs turn it to, and Faber never sends one outside
// them. The defaults are the Servo library's (Servo.h: MIN_PULSE_WIDTH 544, MAX_PULSE_WIDTH
// 2400 us for 0 and 180 degrees); a servo that buzzes at its ends wants a narrower range. The
// library takes minPulseUs within 36..1056 and maxPulseUs within 1892..2908 (Servo.cpp keeps them
// as (544 - min) / 4 and (2400 - max) / 4 in an int8_t).
//
// It starts released: no pulses until Faber sends the first angle, so it doesn't jump at power-up.
// When its lease ends (1 s without a command), on the e-stop and when Faber goes quiet, it holds
// its angle (faberlink::SERVO_HOLD, the default) or goes limp (faberlink::SERVO_RELEASE).
//
// Power: a servo moving draws far more than an Arduino's 5V pin should give (an SG90 several
// hundred mA when it stalls, an MG996R amperes): give it its own 5-6 V supply and join the
// grounds.

#ifndef FABER_SERVO_H
#define FABER_SERVO_H

#include <Servo.h>

#include "FaberLink.h"

class FaberServo : public faberlink::ServoPart {
 public:
  FaberServo(const char* name, uint8_t pin, float minDeg = 0, float maxDeg = 180,
             faberlink::ServoSafeState safeState = faberlink::SERVO_HOLD, uint16_t minPulseUs = 544,
             uint16_t maxPulseUs = 2400)
      : faberlink::ServoPart(name, minDeg * 0.0174532925f, maxDeg * 0.0174532925f, safeState),  // pi / 180
        minUs_(minPulseUs),
        maxUs_(maxPulseUs),
        pin_(pin) {
    faberlink::autoAdd(*this);
  }

 protected:
  uint8_t writePins(uint8_t* out) const override {
    out[0] = faberlink::ROLE_SIGNAL;
    out[1] = pin_;
    return 2;
  }

  // The pulse for `rad`, in a straight line from minPulseUs at the smallest angle to maxPulseUs
  // at the largest.
  void moveTo(float rad) override {
    const float span = maxRad_ - minRad_;
    const float share = span > 0 ? (rad - minRad_) / span : 0.5f;
    const int us = static_cast<int>(minUs_ + share * (maxUs_ - minUs_) + 0.5f);
    if (!servo_.attached()) servo_.attach(pin_, minUs_, maxUs_);
    servo_.writeMicroseconds(us);
  }

  void letGo() override { servo_.detach(); }

 private:
  Servo servo_;
  uint16_t minUs_, maxUs_;
  uint8_t pin_;
};

#endif
