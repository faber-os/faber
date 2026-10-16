// A stand-in for Pololu's VL53L0X library (its API as its README documents it), just big enough
// to compile FaberToF.h on the PC. glue.cpp implements it on simulated sensors: each is on while
// its XSHUT pin isn't driven LOW, answers at its address (0x29 after power-up), and measures a
// range a test sets. Test code: nothing here runs on a board.

#ifndef FABER_FAKE_VL53L0X_H
#define FABER_FAKE_VL53L0X_H

#include <stdint.h>

class VL53L0X {
 public:
  VL53L0X() : sensor_(-1), timeout_(0), timedOut_(false) {}
  bool init(bool io2v8 = true);
  void setAddress(uint8_t newAddress);
  void setTimeout(uint16_t timeout) { timeout_ = timeout; }
  uint16_t getTimeout() const { return timeout_; }
  void startContinuous(uint32_t periodMs = 0);
  uint16_t readRangeContinuousMillimeters();
  bool timeoutOccurred() {
    bool t = timedOut_;
    timedOut_ = false;
    return t;
  }

 private:
  int sensor_;  // which simulated sensor it talks to
  uint16_t timeout_;
  bool timedOut_;
};

#endif
