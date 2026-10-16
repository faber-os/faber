// FaberToF: a VL53L0X laser time-of-flight distance sensor (distance@1, kind laser-tof) on I2C,
// on Pololu's VL53L0X library (github.com/pololu/vl53l0x-arduino, MIT; Library Manager:
// "VL53L0X" by Pololu). An optional header: include it only in sketches with a ToF sensor.
//
//   #include <FaberLink.h>
//   #include <FaberToF.h>
//
//   FaberToF front("front");                                    // one sensor: XSHUT not needed
//   FaberToF left("left", faberlink::TOF_ADDRESS_30, 4);        // several: an address each,
//   FaberToF right("right", faberlink::TOF_ADDRESS_31, 5);      // and an XSHUT pin each
//
// Every VL53L0X starts at address 0x29, and forgets any other at power-up. So with several on one
// bus, every boot the library holds all their XSHUT pins low, then releases them one at a time,
// starting each and giving it its address (docs/research/common-parts.md §2.1). A sensor without
// an XSHUT pin keeps 0x29: at most one may. XSHUT is released by making the pin an input, never
// driven HIGH: on some carriers (Pololu's) XSHUT is not level-shifted and takes 3.3 V at most;
// the carrier's own pull-up raises it.
//
// It measures continuously and reads the newest range every 50 ms; a read waits at most 40 ms
// (a time-out) and then reports "no target". 0.03 m to 2 m: "up to 2 m" is ST's datasheet
// figure (VL53L0X datasheet DocID029104); the 3 cm floor and the 25 degree field of view are the
// usual figures for it (general knowledge, verify on your sensor).
//
// NOT COMPILED HERE against the real library: it is written to Pololu's documented API (init(),
// setAddress(), setTimeout(), startContinuous(), readRangeContinuousMillimeters(),
// timeoutOccurred()), and tested on the PC against a stand-in with that API.

#ifndef FABER_TOF_H
#define FABER_TOF_H

#include <VL53L0X.h>
#include <Wire.h>

#include "FaberLink.h"

namespace faberlink {

// The address a FaberToF gets at every boot (through its XSHUT pin).
enum ToFAddress : uint8_t {
  TOF_ADDRESS_29 = 0x29,  // the sensor's own: for one sensor without an XSHUT pin
  TOF_ADDRESS_30 = 0x30,
  TOF_ADDRESS_31 = 0x31,
  TOF_ADDRESS_32 = 0x32,
  TOF_ADDRESS_33 = 0x33,
  TOF_ADDRESS_34 = 0x34,
  TOF_ADDRESS_35 = 0x35,
  TOF_ADDRESS_36 = 0x36,
  TOF_ADDRESS_37 = 0x37,
};

}  // namespace faberlink

class FaberToF : public faberlink::Distance {
 public:
  explicit FaberToF(const char* name, faberlink::ToFAddress address = faberlink::TOF_ADDRESS_29,
                    uint8_t xshutPin = faberlink::NO_PIN, float minM = 0.03f, float maxM = 2.0f)
      // 0.436 rad: a 25 degree cone. A reading every 50 ms.
      : faberlink::Distance(name, faberlink::DISTANCE_LASER_TOF, minM, maxM, 0.436f, 50),
        next_(first()),
        xshut_(xshutPin),
        address_(address),
        ok_(false) {
    first() = this;
    faberlink::autoAdd(*this);
  }

  // The first sensor's begin() starts them all: every XSHUT low, then one at a time. It takes
  // the list, so the others' begin() finds it empty.
  void begin() override {
    FaberToF* all = first();
    if (!all) return;
    first() = 0;
    Wire.begin();
#if defined(ARDUINO_ARCH_AVR)
    Wire.setWireTimeout(3000, true);  // a stuck bus never hangs loop() (AVR core 1.8.3 and later)
#endif
    for (FaberToF* t = all; t; t = t->next_) {
      if (t->xshut_ == faberlink::NO_PIN) continue;
      digitalWrite(t->xshut_, LOW);
      pinMode(t->xshut_, OUTPUT);
    }
    delay(10);  // every sensor off before any starts
    for (FaberToF* t = all; t; t = t->next_) t->start();
  }

  // Whether it answered when it was started.
  bool ready() const { return ok_; }

 protected:
  uint8_t writePins(uint8_t* out) const override {
    uint8_t n = 0;
#if defined(PIN_WIRE_SDA) && defined(PIN_WIRE_SCL)
    out[n++] = faberlink::ROLE_SDA;
    out[n++] = PIN_WIRE_SDA;
    out[n++] = faberlink::ROLE_SCL;
    out[n++] = PIN_WIRE_SCL;
#endif
    if (xshut_ != faberlink::NO_PIN) {
      out[n++] = faberlink::ROLE_XSHUT;
      out[n++] = xshut_;
    }
    return n;
  }

  float measure() override {
    if (!ok_) return -1;
    const uint16_t mm = sensor_.readRangeContinuousMillimeters();
    // A time-out, or the sensor's "out of range" (8190 and up), is no target.
    if (sensor_.timeoutOccurred() || mm >= 8190) return -1;
    return mm / 1000.0f;
  }

 private:
  static FaberToF*& first() {
    static FaberToF* head = 0;
    return head;
  }

  // Releases its XSHUT (an input: the carrier's pull-up raises it), lets it boot, starts it and
  // gives it its address.
  void start() {
    if (xshut_ != faberlink::NO_PIN) {
      pinMode(xshut_, INPUT);
      delay(10);  // time to boot, with room to spare
    }
    sensor_.setTimeout(40);
    ok_ = sensor_.init();
    if (!ok_) return;
    if (address_ != faberlink::TOF_ADDRESS_29) sensor_.setAddress(address_);
    sensor_.startContinuous();
  }

  VL53L0X sensor_;
  FaberToF* next_;
  uint8_t xshut_;
  uint8_t address_;
  bool ok_;
};

#endif
