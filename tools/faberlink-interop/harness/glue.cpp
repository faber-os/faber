// A whole sketch on the PC: the FaberLink library's Arduino glue (FaberLink.cpp, with FaberMotor,
// FaberDistance, FaberUltrasonic and FaberBattery) on the stand-in Arduino core in arduino/, behind
// a tiny C ABI. Bytes and a fake millis() go in; the board's bytes and every pin it wrote come out.
// This file is test code: unlike the library, it may allocate.

#include <deque>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

#include "Arduino.h"
#include "FaberLink.h"

namespace {

// What a pin was last set to. kind: 0 never written, 1 digitalWrite (value 0/1), 2 analogWrite
// (value 0..255).
struct PinState {
  uint8_t mode;
  uint8_t kind;
  int32_t value;
};

struct PinWrite {
  uint8_t pin;
  uint8_t kind;
  int32_t value;
};

struct Pins {
  Pins() : nowMs(0), echoUs(0) {
    for (int i = 0; i < 256; i++) {
      state[i].mode = 0xFF;  // never set
      state[i].kind = 0;
      state[i].value = 0;
      analogIn[i] = 0;
      digitalIn[i] = -1;
    }
  }
  PinState state[256];
  int analogIn[256];
  // What drives an input pin from outside: 0 LOW, 1 HIGH, -1 nothing (it floats, or its pull-up
  // holds it HIGH).
  int digitalIn[256];
  std::vector<PinWrite> log;
  unsigned long nowMs;
  unsigned long echoUs;
};

// The sketch the current call is about. Every entry point sets it; tests drive one sketch from
// one thread at a time.
thread_local Pins* current = 0;

class FakeSerial : public HardwareSerial {
 public:
  FakeSerial() : rxPos(0), baud(0) {}
  void begin(unsigned long b) override { baud = b; }
  int available() override { return static_cast<int>(rx.size() - rxPos); }
  int read() override { return rxPos < rx.size() ? rx[rxPos++] : -1; }
  size_t write(const uint8_t* data, size_t len) override {
    tx.insert(tx.end(), data, data + len);
    return len;
  }
  std::vector<uint8_t> rx, tx;
  size_t rxPos;
  unsigned long baud;
};

// One part of a sketch, as the Rust side describes it. type: 0 FaberMotor(name, pwm, dir, max),
// 1 FaberMotor(name, driver, pins[0..4]), 2 FaberDistance(name, pin), 3 FaberUltrasonic(name,
// trig, echo), 4 FaberBattery(name, pin, 3.0, 3), 5 FaberButton(name, pin, driver as the active
// level), 6 FaberObstacle(name, pin, pins[1] as the kind, driver as the active level,
// maxVelocity as the trigger distance), 7 FaberOutput(name, pin, driver as the mode, pins[1] as
// the active level).
struct PartSpec {
  uint8_t type;
  const char* name;
  uint8_t driver;
  uint8_t pins[4];
  float maxVelocity;
};

struct Sketch {
  Sketch() : link(serial) {}
  Pins pins;
  FakeSerial serial;
  FaberLink link;
  std::deque<std::string> names;  // parts keep a pointer to their name
  std::vector<std::unique_ptr<FaberMotor>> motors;
  std::vector<std::unique_ptr<FaberDistance>> distances;
  std::vector<std::unique_ptr<FaberUltrasonic>> ultrasonics;
  std::vector<std::unique_ptr<FaberBattery>> batteries;
  std::vector<std::unique_ptr<FaberButton>> buttons;
  std::vector<std::unique_ptr<FaberObstacle>> obstacles;
  std::vector<std::unique_ptr<FaberOutput>> outputs;
};

// Parts register themselves in a global table until the next FaberLink::begin(), as globals in a
// sketch do: building a sketch is one step at a time across threads.
std::mutex building;

Sketch* use(void* h) {
  Sketch* s = static_cast<Sketch*>(h);
  current = &s->pins;
  return s;
}

void record(uint8_t pin, uint8_t kind, int32_t value) {
  if (!current) return;
  current->state[pin].kind = kind;
  current->state[pin].value = value;
  PinWrite w = {pin, kind, value};
  current->log.push_back(w);
}

}  // namespace

void pinMode(uint8_t pin, uint8_t mode) {
  if (current) current->state[pin].mode = mode;
}

void digitalWrite(uint8_t pin, uint8_t value) { record(pin, 1, value ? 1 : 0); }

// An output reads what was written to it; an input what drives it, or HIGH from its pull-up.
int digitalRead(uint8_t pin) {
  if (!current) return 0;
  const PinState& s = current->state[pin];
  if (s.mode == OUTPUT) return s.kind == 1 ? s.value : 0;
  if (current->digitalIn[pin] >= 0) return current->digitalIn[pin];
  return s.mode == INPUT_PULLUP ? 1 : 0;
}

// As on an AVR: 0 and 255 are plain digital levels there too, but the value is what the sketch
// asked for, which is what the tests check.
void analogWrite(uint8_t pin, int value) { record(pin, 2, value < 0 ? 0 : value > 255 ? 255 : value); }

int analogRead(uint8_t pin) { return current ? current->analogIn[pin] : 0; }

unsigned long millis() { return current ? current->nowMs : 0; }

void delayMicroseconds(unsigned int) {}

unsigned long pulseIn(uint8_t, uint8_t, unsigned long timeoutUs) {
  if (!current || current->echoUs > timeoutUs) return 0;
  return current->echoUs;
}

extern "C" {

void* fla_new(const PartSpec* parts, size_t n) {
  std::lock_guard<std::mutex> lock(building);
  Sketch* s = new Sketch();
  use(s);
  for (size_t i = 0; i < n; i++) {
    const PartSpec& p = parts[i];
    s->names.push_back(p.name);
    const char* name = s->names.back().c_str();
    switch (p.type) {
      case 0:
        s->motors.emplace_back(new FaberMotor(name, p.pins[0], p.pins[1], p.maxVelocity));
        break;
      case 1: {
        // The shortest call that says it, as a sketch writes it: this also checks that
        // FaberMotor("x", faberlink::DRIVER_TWO_PWM, 5, 6) picks the driver constructor, not
        // (pwm, dir, maxVelocity).
        faberlink::MotorDriver driver = static_cast<faberlink::MotorDriver>(p.driver);
        FaberMotor* m;
        if (p.pins[2] == faberlink::NO_PIN && p.pins[3] == faberlink::NO_PIN) {
          m = new FaberMotor(name, driver, p.pins[0], p.pins[1]);
        } else if (p.pins[3] == faberlink::NO_PIN) {
          m = new FaberMotor(name, driver, p.pins[0], p.pins[1], p.pins[2]);
        } else {
          m = new FaberMotor(name, driver, p.pins[0], p.pins[1], p.pins[2], p.pins[3]);
        }
        s->motors.emplace_back(m);
        if (p.maxVelocity > 0) m->setMaxVelocity(p.maxVelocity);
        break;
      }
      case 2:
        s->distances.emplace_back(new FaberDistance(name, p.pins[0]));
        break;
      case 3:
        s->ultrasonics.emplace_back(new FaberUltrasonic(name, p.pins[0], p.pins[1]));
        break;
      case 5:
        if (p.driver == faberlink::ACTIVE_LOW) {
          s->buttons.emplace_back(new FaberButton(name, p.pins[0]));  // the shortest call
        } else {
          s->buttons.emplace_back(new FaberButton(name, p.pins[0], static_cast<faberlink::ActiveLevel>(p.driver)));
        }
        break;
      case 6:
        s->obstacles.emplace_back(new FaberObstacle(name, p.pins[0], static_cast<faberlink::ObstacleKind>(p.pins[1]),
                                                    static_cast<faberlink::ActiveLevel>(p.driver), p.maxVelocity));
        break;
      case 7:
        if (p.driver == faberlink::OUTPUT_ON_OFF && p.pins[1] == faberlink::ACTIVE_HIGH) {
          s->outputs.emplace_back(new FaberOutput(name, p.pins[0]));  // the shortest call
        } else {
          s->outputs.emplace_back(new FaberOutput(name, p.pins[0], static_cast<faberlink::OutputMode>(p.driver),
                                                  static_cast<faberlink::ActiveLevel>(p.pins[1])));
        }
        break;
      default:
        s->batteries.emplace_back(new FaberBattery(name, p.pins[0], 3.0f, 3));
        break;
    }
  }
  s->link.begin(115200, "sketch", "0.1.0");
  current = 0;
  return s;
}

void fla_free(void* h) {
  delete static_cast<Sketch*>(h);
  current = 0;
}

void fla_feed(void* h, const uint8_t* data, size_t len, uint32_t now_ms) {
  Sketch* s = use(h);
  s->serial.rx.insert(s->serial.rx.end(), data, data + len);
  s->pins.nowMs = now_ms;
  s->link.poll();
}

void fla_tick(void* h, uint32_t now_ms) {
  Sketch* s = use(h);
  s->pins.nowMs = now_ms;
  s->link.poll();
}

int64_t fla_take(void* h, uint8_t* buf, size_t cap) {
  Sketch* s = use(h);
  if (s->serial.tx.size() > cap) return -1;
  size_t n = s->serial.tx.size();
  for (size_t i = 0; i < n; i++) buf[i] = s->serial.tx[i];
  s->serial.tx.clear();
  return static_cast<int64_t>(n);
}

uint32_t fla_baud(void* h) { return static_cast<uint32_t>(use(h)->serial.baud); }

void fla_pin(void* h, uint8_t pin, uint8_t* mode, uint8_t* kind, int32_t* value) {
  const PinState& p = use(h)->pins.state[pin];
  *mode = p.mode;
  *kind = p.kind;
  *value = p.value;
}

size_t fla_log_len(void* h) { return use(h)->pins.log.size(); }

void fla_log_at(void* h, size_t i, uint8_t* pin, uint8_t* kind, int32_t* value) {
  const PinWrite& w = use(h)->pins.log[i];
  *pin = w.pin;
  *kind = w.kind;
  *value = w.value;
}

void fla_clear_log(void* h) { use(h)->pins.log.clear(); }

void fla_set_analog(void* h, uint8_t pin, int32_t value) { use(h)->pins.analogIn[pin] = value; }

void fla_set_echo_us(void* h, uint32_t us) { use(h)->pins.echoUs = us; }

void fla_set_digital(void* h, uint8_t pin, int32_t level) { use(h)->pins.digitalIn[pin] = level; }
}
