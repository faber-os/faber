// A stand-in for the Arduino core, just big enough to compile the FaberLink library's Arduino glue
// (firmware/faberlink/src/FaberLink.cpp) on the PC. Pins are numbered as on an Uno (A0 is 14).
// What a sketch writes to its pins is recorded per sketch, so a test can check it pin by pin
// (glue.cpp). Test code: nothing here runs on a board.

#ifndef FABER_FAKE_ARDUINO_H
#define FABER_FAKE_ARDUINO_H

#include <math.h>
#include <stddef.h>
#include <stdint.h>

#define HIGH 0x1
#define LOW 0x0
#define INPUT 0x0
#define OUTPUT 0x1
#define INPUT_PULLUP 0x2

#define PIN_A0 (14)
#define PIN_A1 (15)
#define PIN_A2 (16)
#define PIN_A3 (17)
#define PIN_A4 (18)
#define PIN_A5 (19)
static const uint8_t A0 = PIN_A0;
static const uint8_t A1 = PIN_A1;
static const uint8_t A2 = PIN_A2;
static const uint8_t A3 = PIN_A3;
static const uint8_t A4 = PIN_A4;
static const uint8_t A5 = PIN_A5;

void pinMode(uint8_t pin, uint8_t mode);
void digitalWrite(uint8_t pin, uint8_t value);
int digitalRead(uint8_t pin);
void analogWrite(uint8_t pin, int value);
int analogRead(uint8_t pin);
unsigned long millis();
void delayMicroseconds(unsigned int us);
unsigned long pulseIn(uint8_t pin, uint8_t state, unsigned long timeoutUs = 1000000UL);

class Stream {
 public:
  virtual int available() = 0;
  virtual int read() = 0;
  virtual size_t write(const uint8_t* data, size_t len) = 0;

 protected:
  ~Stream() {}
};

class HardwareSerial : public Stream {
 public:
  virtual void begin(unsigned long baud) = 0;

 protected:
  ~HardwareSerial() {}
};

#endif
