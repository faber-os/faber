// FaberEncoder: a quadrature encoder (encoder@1) counted with interrupts: a wheel encoder on a
// gear motor, a rotary knob (KY-040). An optional header, so only a sketch with an encoder carries
// interrupt routines (a sketch that has none keeps the bytes and the two pins' interrupts).
//
//   #include <FaberLink.h>
//   #include <FaberEncoder.h>
//
//   FaberEncoder wheel("wheel", 2, 3, 11, 34.02f);  // 11 pulses a turn, a 34.02:1 gearbox
//   FaberEncoder knob("knob", 2, 4);                // a KY-040: counts only
//
// Channel A must be on an external-interrupt pin (an Uno has 2 and 3; a Mega 2, 3, 18, 19, 20
// and 21, where 18/19 are also Serial1 and 20/21 also I2C). With B on one too it counts every
// edge of both channels (4 counts a pulse); with B on any other pin, the edges of A (2 counts a
// pulse). Both pins get their pull-ups (hall encoders often have open-collector outputs).
//
// pulsesPerRev is the encoder's pulses for one turn of the motor, gearRatio what the gearbox
// turns that into at the shaft you care about: with both, Faber gets the speed in rad/s, and a
// FaberMotor can report it (left.useEncoder(wheel) in setup()). At most 4 encoders on a board.
// Forward (counting up) is A changing before B; swap A and B to count the other way.
//
// Each edge is an interrupt of a few microseconds: the total edge rate of all encoders has a
// ceiling in the tens of kHz on an AVR (docs/research/common-parts.md §2.2: measure before relying
// on it).

#ifndef FABER_ENCODER_H
#define FABER_ENCODER_H

#include "FaberLink.h"

class FaberEncoder;

namespace faberlink {
namespace encoders {

const uint8_t MAX = 4;

// The encoder each of the four interrupt routines is for: an interrupt can't say which it is.
inline FaberEncoder*& slot(uint8_t i) {
  static FaberEncoder* slots[MAX];
  return slots[i];
}

// The step from the last state of (A, B) to the new one, at last * 4 + new: +1 forward (A changes
// first), -1 backward, 0 for no change or a skipped state.
inline int8_t step(uint8_t last, uint8_t now) {
  static const int8_t QUADRATURE[16] = {0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0};
  return QUADRATURE[(last << 2) | now];
}

}  // namespace encoders
}  // namespace faberlink

class FaberEncoder : public faberlink::Encoder {
 public:
  FaberEncoder(const char* name, uint8_t pinA, uint8_t pinB, float pulsesPerRev = 0, float gearRatio = 1)
      : faberlink::Encoder(name),
        pulsesPerRev_(pulsesPerRev),
        gearRatio_(gearRatio),
        count_(0),
        a_(pinA),
        b_(pinB),
        last_(0),
        perPulse_(0) {
    faberlink::autoAdd(*this);
  }

  void begin() override {
    pinMode(a_, INPUT_PULLUP);
    pinMode(b_, INPUT_PULLUP);
    last_ = state();
    const int irqA = digitalPinToInterrupt(a_);
    if (irqA == NOT_AN_INTERRUPT) return;  // it can't count: A needs an interrupt pin
    uint8_t i = 0;
    while (i < faberlink::encoders::MAX && faberlink::encoders::slot(i)) i++;
    if (i == faberlink::encoders::MAX) return;
    faberlink::encoders::slot(i) = this;
    static void (*const EDGES[faberlink::encoders::MAX])() = {edge<0>, edge<1>, edge<2>, edge<3>};
    const int irqB = digitalPinToInterrupt(b_);
    perPulse_ = irqB == NOT_AN_INTERRUPT ? 2 : 4;
    attachInterrupt(static_cast<uint8_t>(irqA), EDGES[i], CHANGE);
    if (perPulse_ == 4) attachInterrupt(static_cast<uint8_t>(irqB), EDGES[i], CHANGE);
    if (pulsesPerRev_ > 0 && gearRatio_ > 0) setCountsPerRev(pulsesPerRev_ * perPulse_ * gearRatio_);
  }

  // Counts since it started. The count is four bytes, changed by an interrupt: read with
  // interrupts off.
  int32_t count() override {
    noInterrupts();
    const int32_t c = count_;
    interrupts();
    return c;
  }

  // 4 with both pins on interrupts, 2 with only A, 0 when A isn't an interrupt pin (it can't count).
  uint8_t countsPerPulse() const { return perPulse_; }

  // Its interrupt: one edge on A or B.
  void onEdge() {
    const uint8_t now = state();
    if (perPulse_ == 4) {
      count_ += faberlink::encoders::step(last_, now);
    } else if ((now ^ last_) & 2) {
      // Only A interrupts: on each of its edges, A differing from B is forward.
      count_ += ((now >> 1) != (now & 1)) ? 1 : -1;
    }
    last_ = now;
  }

 protected:
  uint8_t writePins(uint8_t* out) const override {
    out[0] = faberlink::ROLE_A;
    out[1] = a_;
    out[2] = faberlink::ROLE_B;
    out[3] = b_;
    return 4;
  }

 private:
  template <uint8_t I>
  static void edge() {
    faberlink::encoders::slot(I)->onEdge();
  }

  uint8_t state() const {
    return static_cast<uint8_t>((digitalRead(a_) == HIGH ? 2 : 0) | (digitalRead(b_) == HIGH ? 1 : 0));
  }

  float pulsesPerRev_, gearRatio_;
  volatile int32_t count_;
  uint8_t a_, b_, last_, perPulse_;
};

#endif
