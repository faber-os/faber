// A stand-in for the Arduino Wire (I2C) library (its API as in the AVR core 1.8.8's Wire.h), just
// big enough to compile FaberImu.h on the PC. glue.cpp implements it on a simulated bus with a
// register file per address, so a test can play an MPU6050. Test code: nothing here runs on a
// board.

#ifndef FABER_FAKE_WIRE_H
#define FABER_FAKE_WIRE_H

#include <stddef.h>
#include <stdint.h>

#define PIN_WIRE_SDA (18)
#define PIN_WIRE_SCL (19)

class TwoWire {
 public:
  void begin();
  void setWireTimeout(uint32_t timeout = 25000, bool resetWithTimeout = false);
  void beginTransmission(uint8_t address);
  size_t write(uint8_t data);
  uint8_t endTransmission(bool stop = true);
  uint8_t requestFrom(uint8_t address, uint8_t quantity);
  int available();
  int read();
};

extern TwoWire Wire;

#endif
