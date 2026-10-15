// A fake board for the interop tests: the real FaberLink protocol core (FaberLinkCore.cpp) with a
// simulated motor and distance sensor, behind a tiny C ABI. Bytes and a fake millis() go in, the
// board's bytes come out. This file is test code: unlike the library, it may allocate.

#include <stdlib.h>
#include <string.h>

#include "FaberLinkCore.h"

namespace {

class BufferOutput : public faberlink::Output {
 public:
  BufferOutput() : len(0), overflowed(false) {}
  void write(const uint8_t* data, size_t n) override {
    if (len + n > sizeof(buf)) {
      overflowed = true;
      return;
    }
    memcpy(buf + len, data, n);
    len += n;
  }
  uint8_t buf[16384];
  size_t len;
  bool overflowed;
};

// A motor whose "hardware" is a variable, reporting its open-loop velocity as a reading.
class SimMotor : public faberlink::Motor {
 public:
  SimMotor(const char* name, uint16_t leaseMs)
      : faberlink::Motor(name, 10.0f, faberlink::MOTOR_DC, leaseMs), driven(0), driveCalls(0) {
    setReadingPeriod(50);
  }
  uint8_t writeReading(uint8_t* out, uint32_t) override {
    out[0] = 1;  // velocity present
    faberlink::putF32(out + 1, driven * maxVelocity_);
    faberlink::putF32(out + 5, 0);
    faberlink::putF32(out + 9, 0);
    return 13;
  }
  float driven;
  int driveCalls;

 protected:
  void drive(float duty) override {
    driven = duty;
    driveCalls++;
  }
};

class SimDistance : public faberlink::Distance {
 public:
  explicit SimDistance(const char* name)
      : faberlink::Distance(name, faberlink::DISTANCE_INFRARED, 0.1f, 0.8f, 0.1f, 50), meters(0.5f) {}
  float meters;

 protected:
  float measure() override { return meters; }
};

// The small board hosts `left` (part 0) and `front` (part 1). The Mega also hosts `right` (part 1)
// and `rear` (part 3): motors left, right, then distances front, rear.
struct Harness {
  Harness(uint16_t heartbeatTimeoutMs, bool mega, uint16_t motorLeaseMs)
      : board(out),
        motor("left", motorLeaseMs),
        motor2("right", motorLeaseMs),
        distance("front"),
        distance2("rear") {
    board.setIdentity(mega ? "sim-mega" : "sim-board", "faberlink-harness", "0.1.0");
    board.setHeartbeatTimeout(heartbeatTimeoutMs);
    board.add(motor);
    if (mega) board.add(motor2);
    board.add(distance);
    if (mega) board.add(distance2);
    board.begin();
  }
  SimMotor& motorAt(int i) { return i == 1 ? motor2 : motor; }
  SimDistance& distanceAt(int i) { return i == 1 ? distance2 : distance; }
  BufferOutput out;
  faberlink::Board board;
  SimMotor motor;
  SimMotor motor2;
  SimDistance distance;
  SimDistance distance2;
};

}  // namespace

extern "C" {

void* flh_new(uint16_t heartbeat_timeout_ms) {
  return new Harness(heartbeat_timeout_ms, false, faberlink::DEFAULT_LEASE_MS);
}

// The Mega; its motors stop by themselves after motor_lease_ms without a command.
void* flh_new_mega(uint16_t heartbeat_timeout_ms, uint16_t motor_lease_ms) {
  return new Harness(heartbeat_timeout_ms, true, motor_lease_ms);
}

// Motor `i` (0 left, 1 right) and distance sensor `i` (0 front, 1 rear).
float flh_motor_output_at(void* h, int i) { return static_cast<Harness*>(h)->motorAt(i).driven; }

void flh_set_distance_at(void* h, int i, float meters) { static_cast<Harness*>(h)->distanceAt(i).meters = meters; }

void flh_free(void* h) { delete static_cast<Harness*>(h); }

void flh_feed(void* h, const uint8_t* data, size_t len, uint32_t now_ms) {
  Harness* x = static_cast<Harness*>(h);
  for (size_t i = 0; i < len; i++) x->board.feed(data[i], now_ms);
}

void flh_tick(void* h, uint32_t now_ms) { static_cast<Harness*>(h)->board.tick(now_ms); }

// Copies out (and clears) what the board has sent. Returns the length, or -1 if the board wrote
// more than the harness buffer holds.
int64_t flh_take(void* h, uint8_t* buf, size_t cap) {
  Harness* x = static_cast<Harness*>(h);
  if (x->out.overflowed || x->out.len > cap) return -1;
  size_t n = x->out.len;
  memcpy(buf, x->out.buf, n);
  x->out.len = 0;
  return static_cast<int64_t>(n);
}

float flh_motor_output(void* h) { return static_cast<Harness*>(h)->motor.driven; }

int flh_motor_drive_calls(void* h) { return static_cast<Harness*>(h)->motor.driveCalls; }

void flh_set_distance(void* h, float meters) { static_cast<Harness*>(h)->distance.meters = meters; }

void flh_hardware_estop(void* h) { static_cast<Harness*>(h)->board.engageHardwareEstop(); }

void flh_reset_hardware_estop(void* h) { static_cast<Harness*>(h)->board.resetHardwareEstop(); }

uint16_t flh_heartbeat_timeout(void* h) { return static_cast<Harness*>(h)->board.heartbeatTimeout(); }

void flh_speak_versions(void* h, uint8_t lo, uint8_t hi) { static_cast<Harness*>(h)->board.speakVersions(lo, hi); }

int flh_estop_engaged(void* h) { return static_cast<Harness*>(h)->board.estopEngaged() ? 1 : 0; }

// Sizes on this machine, for the RAM budget (a pointer is 8 bytes here, 2 on an AVR).
size_t flh_sizeof_board() { return sizeof(faberlink::Board); }
size_t flh_max_payload() { return FABERLINK_MAX_PAYLOAD; }
}
