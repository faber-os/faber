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

class SimBattery : public faberlink::Battery {
 public:
  SimBattery(faberlink::BatteryChemistry chemistry, uint8_t cells, float capacityAh)
      : faberlink::Battery("battery", chemistry, cells, capacityAh, 100), volts(12.0f) {}
  float volts;

 protected:
  float measureVolts() override { return volts; }
};

class SimDistance : public faberlink::Distance {
 public:
  explicit SimDistance(const char* name)
      : faberlink::Distance(name, faberlink::DISTANCE_INFRARED, 0.1f, 0.8f, 0.1f, 50), meters(0.5f) {}
  float meters;

 protected:
  float measure() override { return meters; }
};

// planar-base@1 the way SENNEP's Mega will host it (docs/faberlink.md). The library has no
// planar-base part type yet, so this test part writes the capability's layouts itself. Holonomic,
// at most 0.6 m/s and 1.5 rad/s; its "hardware" is the velocity it was last given, which it also
// reports as measured. Its safe state is standing still.
class SimPlanarBase : public faberlink::Part {
 public:
  SimPlanarBase(const char* name, uint16_t leaseMs)
      : faberlink::Part(name, faberlink::PLANAR_BASE, leaseMs, 50), vx(0), vy(0), omega(0) {}
  uint8_t writeInfo(uint8_t* out) const override {
    out[0] = 1;  // holonomic
    putTwist(out + 1, kMaxV, kMaxV, kMaxOmega);
    putTwist(out + 13, 1.0f, 1.0f, 3.0f);  // max acceleration
    out[25] = 0;                           // built from: nothing named
    return 26;
  }
  uint8_t writeReading(uint8_t* out, uint32_t) override {
    out[0] = 1;  // measured is known; not bumped
    putTwist(out + 1, vx, vy, omega);
    putTwist(out + 13, vx, vy, omega);
    return 25;
  }
  uint8_t apply(const uint8_t* c, uint8_t len, uint8_t* out, bool* clamped) override {
    if (len != 13 || c[0] != 0) return 0;  // only set-velocity
    float v[3] = {faberlink::getF32(c + 1), faberlink::getF32(c + 5), faberlink::getF32(c + 9)};
    const float max[3] = {kMaxV, kMaxV, kMaxOmega};
    for (int i = 0; i < 3; i++) {
      if (!(v[i] == v[i]) || v[i] > 1e30f || v[i] < -1e30f) return 0;  // NaN or infinite
      if (v[i] > max[i]) {
        v[i] = max[i];
        *clamped = true;
      } else if (v[i] < -max[i]) {
        v[i] = -max[i];
        *clamped = true;
      }
    }
    vx = v[0];
    vy = v[1];
    omega = v[2];
    out[0] = 0;
    putTwist(out + 1, vx, vy, omega);
    return 13;
  }
  uint8_t enterSafeState(uint8_t* out) override {
    vx = vy = omega = 0;
    out[0] = 0;
    putTwist(out + 1, 0, 0, 0);
    return 13;
  }
  float vx, vy, omega;

 private:
  static const float kMaxV;
  static const float kMaxOmega;
  static void putTwist(uint8_t* p, float a, float b, float c) {
    faberlink::putF32(p, a);
    faberlink::putF32(p + 4, b);
    faberlink::putF32(p + 8, c);
  }
};

const float SimPlanarBase::kMaxV = 0.6f;
const float SimPlanarBase::kMaxOmega = 1.5f;

// The small board hosts `left` (part 0) and `front` (part 1), and with a battery `battery` (part 2).
// The Mega also hosts `right` (part 1) and `rear` (part 3): motors left, right, then distances
// front, rear. SENNEP's Mega hosts a planar base `base` (part 0), distances `front` and `rear`
// (parts 1 and 2) and a 3S Li-ion `battery` (part 3).
enum Kind { SMALL, MEGA, SENNEP };

struct Harness {
  Harness(uint16_t heartbeatTimeoutMs, Kind kind, uint16_t leaseMs, bool withBattery = false,
          faberlink::BatteryChemistry chemistry = faberlink::BATTERY_LI_ION, uint8_t cells = 3,
          float capacityAh = 0)
      : board(out),
        motor("left", leaseMs),
        motor2("right", leaseMs),
        distance("front"),
        distance2("rear"),
        battery(chemistry, cells, capacityAh),
        base("base", leaseMs) {
    board.setIdentity(kind == SENNEP ? "sim-sennep-mega" : kind == MEGA ? "sim-mega" : "sim-board",
                      "faberlink-harness", "0.1.0");
    board.setHeartbeatTimeout(heartbeatTimeoutMs);
    if (kind == SENNEP) {
      board.add(base);
      board.add(distance);
      board.add(distance2);
      board.add(battery);
    } else {
      board.add(motor);
      if (kind == MEGA) board.add(motor2);
      board.add(distance);
      if (kind == MEGA) board.add(distance2);
      if (withBattery) board.add(battery);
    }
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
  SimBattery battery;
  SimPlanarBase base;
};

}  // namespace

extern "C" {

void* flh_new(uint16_t heartbeat_timeout_ms) {
  return new Harness(heartbeat_timeout_ms, SMALL, faberlink::DEFAULT_LEASE_MS);
}

// The Mega; its motors stop by themselves after motor_lease_ms without a command.
void* flh_new_mega(uint16_t heartbeat_timeout_ms, uint16_t motor_lease_ms) {
  return new Harness(heartbeat_timeout_ms, MEGA, motor_lease_ms);
}

// SENNEP's Mega; its base stops by itself after base_lease_ms without a command.
void* flh_new_sennep(uint16_t heartbeat_timeout_ms, uint16_t base_lease_ms) {
  return new Harness(heartbeat_timeout_ms, SENNEP, base_lease_ms);
}

// The velocity SENNEP's base is driven at now: vx, vy and omega into out[0..3].
void flh_base_velocity(void* h, float* out) {
  const SimPlanarBase& b = static_cast<Harness*>(h)->base;
  out[0] = b.vx;
  out[1] = b.vy;
  out[2] = b.omega;
}

// The small board with a battery (part 2) of this chemistry (a faberlink::BatteryChemistry),
// cell count and capacity (0 = unknown). It reports every 100 ms.
void* flh_new_with_battery(uint16_t heartbeat_timeout_ms, uint8_t chemistry, uint8_t cells, float capacity_ah) {
  return new Harness(heartbeat_timeout_ms, SMALL, faberlink::DEFAULT_LEASE_MS, true,
                     static_cast<faberlink::BatteryChemistry>(chemistry), cells, capacity_ah);
}

void flh_set_battery_volts(void* h, float volts) { static_cast<Harness*>(h)->battery.volts = volts; }

// The library's own level for `volts`, coming from `previous`.
uint8_t flh_battery_level_at(void* h, float volts, uint8_t previous) {
  return static_cast<Harness*>(h)->battery.levelAt(volts, static_cast<faberlink::BatteryLevel>(previous));
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
