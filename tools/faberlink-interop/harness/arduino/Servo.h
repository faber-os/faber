// A stand-in for the Arduino Servo library (its API as in Servo 1.3.0's src/Servo.h), just big
// enough to compile FaberServo.h on the PC. glue.cpp implements it and records, pin by pin, the
// pulse a servo is sent, or that it gets none. Test code: nothing here runs on a board.

#ifndef FABER_FAKE_SERVO_H
#define FABER_FAKE_SERVO_H

#include <stdint.h>

class Servo {
 public:
  Servo() : pin_(-1), attached_(false) {}
  uint8_t attach(int pin);
  uint8_t attach(int pin, int min, int max);
  void detach();
  void write(int value);
  void writeMicroseconds(int value);
  bool attached() { return attached_; }

 private:
  int pin_;
  bool attached_;
};

#endif
