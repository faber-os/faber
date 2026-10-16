// FaberLink protocol core: plain, portable C++11.
//
// No Arduino headers, no dynamic allocation, no exceptions, no RTTI. The same file is compiled
// for the board (by the Arduino IDE) and on the PC (by the Rust interop tests), so what runs on
// the board is exactly what was tested. The wire format is in docs/faberlink.md.
//
// The board-side safety rules live here, not in the sketch (docs/faberlink.md §5):
//   1. No HEARTBEAT for the heartbeat timeout: every actuator goes to its safe state.
//      Only a new COMMAND starts it again; a heartbeat alone never does.
//   2. An actuator that gets no COMMAND for its own lease time (250 ms) stops by itself.
//   3. A latched hardware e-stop wins until it is reset on the board AND released in Faber.
//   4. Commands are only applied while the heartbeat is alive and no e-stop is engaged.

#ifndef FABERLINK_CORE_H
#define FABERLINK_CORE_H

#include <stddef.h>
#include <stdint.h>

// The largest payload this board accepts and sends. Boards with 2-2.5 KB of RAM (Uno, Nano,
// Leonardo) default to 64 bytes, enough for every message a few parts need; others take the full
// 250. A sketch may set it before including FaberLink.h, but only through the build flags
// (-DFABERLINK_MAX_PAYLOAD=...), so that every file sees the same value.
#ifndef FABERLINK_MAX_PAYLOAD
#if defined(__AVR_ATmega328P__) || defined(__AVR_ATmega328__) || defined(__AVR_ATmega168__) || \
    defined(__AVR_ATmega32U4__)
#define FABERLINK_MAX_PAYLOAD 64
#else
#define FABERLINK_MAX_PAYLOAD 250
#endif
#endif

#if FABERLINK_MAX_PAYLOAD < 32 || FABERLINK_MAX_PAYLOAD > 250
#error "FABERLINK_MAX_PAYLOAD must be 32..250"
#endif

namespace faberlink {

const uint8_t PROTOCOL_VERSION = 1;
const uint8_t MIN_VERSION = 1;
const uint8_t MAX_VERSION = 1;
const uint16_t DEFAULT_HEARTBEAT_TIMEOUT_MS = 250;
const uint16_t MIN_HEARTBEAT_TIMEOUT_MS = 100;
const uint16_t MAX_HEARTBEAT_TIMEOUT_MS = 1000;
const uint16_t DEFAULT_LEASE_MS = 250;
// digital-out@1, pwm-out@1 and servo@1 are leased for 1 s (docs/capabilities.md).
const uint16_t OUTPUT_LEASE_MS = 1000;
const uint16_t STATUS_PERIOD_MS = 100;
const size_t MAX_RAW = FABERLINK_MAX_PAYLOAD + 4;  // type, seq, payload, CRC

// Message types.
namespace kind {
const uint8_t HELLO = 0x01;
const uint8_t HELLO_ACK = 0x02;
const uint8_t DESCRIBE = 0x03;
const uint8_t PART = 0x04;
const uint8_t PINS = 0x05;
const uint8_t HEARTBEAT = 0x10;
const uint8_t STATUS = 0x11;
const uint8_t READING = 0x20;
const uint8_t COMMAND = 0x21;
const uint8_t APPLIED = 0x22;
const uint8_t SAFE = 0x23;
const uint8_t ESTOP = 0x30;
const uint8_t LOG = 0x40;
}  // namespace kind

// Capability codes in READING, COMMAND and APPLIED.
enum Capability : uint8_t {
  DISTANCE = 1,
  MOTOR = 2,
  PLANAR_BASE = 3,
  ESTOP_CAP = 4,
  BATTERY = 5,
  DIGITAL_IN = 6,
  OBSTACLE = 7,
  DIGITAL_OUT = 8,
  PWM_OUT = 9,
  SERVO = 10,
};

// Why an actuator's output changed (APPLIED.cause).
enum Cause : uint8_t {
  CAUSE_COMMAND = 0,
  CAUSE_REFUSED = 1,
  CAUSE_HEARTBEAT_LOST = 2,
  CAUSE_LEASE_EXPIRED = 3,
  CAUSE_SAFE = 4,
  CAUSE_ESTOP = 5,
  CAUSE_NO_HEARTBEAT = 6,
};

enum Fault : uint8_t {
  FAULT_NONE = 0,
  FAULT_HEARTBEAT_LOST = 1,
  FAULT_LEASE_EXPIRED = 2,
  FAULT_BAD_FRAMES = 3,
  FAULT_REFUSED_COMMAND = 4,
  FAULT_ESTOP = 5,
};

enum LogLevel : uint8_t { LOG_INFO = 0, LOG_WARN = 1, LOG_ERROR = 2 };

// What a pin does for its part (PINS). One table for every part class, so Faber can name a pin
// without knowing the class. New roles get new codes; an older Faber shows them as a plain pin.
enum PinRole : uint8_t {
  ROLE_PWM = 1,     // a motor driver's speed input, beside a direction pin
  ROLE_DIR = 2,     // a motor driver's direction input
  ROLE_EN = 3,      // a motor driver's enable input (PWM), beside IN1 and IN2
  ROLE_IN1 = 4,     // a motor driver's first input
  ROLE_IN2 = 5,     // a motor driver's second input
  ROLE_STBY = 6,    // a motor driver's standby input
  ROLE_TRIG = 7,    // a range finder's trigger
  ROLE_ECHO = 8,    // a range finder's echo
  ROLE_SIGNAL = 9,  // a sensor's output read by the board (also a button's or a module's pin)
  ROLE_SENSE = 10,  // a battery's voltage, through a divider
  ROLE_OUT = 11,    // an output the board switches: an LED, a relay module, a MOSFET
  ROLE_A = 12,      // a quadrature encoder's channel A
  ROLE_B = 13,      // a quadrature encoder's channel B
  ROLE_SDA = 14,    // the I2C bus's data line, for a part on the bus
  ROLE_SCL = 15,    // the I2C bus's clock line
  ROLE_XSHUT = 16,  // a time-of-flight sensor's shutdown input (VL53L0X XSHUT)
};

// "No pin": an optional pin that isn't wired, or (as the analog base) "not known".
const uint8_t NO_PIN = 0xFF;

// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF).
uint16_t crc16(const uint8_t* data, size_t len, uint16_t crc = 0xFFFF);

// COBS-decodes buf[0..len) in place. Returns the decoded length, or -1 if it isn't valid COBS.
int cobsDecode(uint8_t* buf, size_t len);

// True for a float that is neither NaN nor infinite (without <math.h>, so it is portable).
bool isFinite(float f);

// Little-endian helpers.
void putU16(uint8_t* p, uint16_t v);
void putU32(uint8_t* p, uint32_t v);
void putF32(uint8_t* p, float v);
uint16_t getU16(const uint8_t* p);
uint32_t getU32(const uint8_t* p);
float getF32(const uint8_t* p);

// Where frames go: a serial port on the board, a buffer in the tests.
class Output {
 public:
  virtual void write(const uint8_t* data, size_t len) = 0;

 protected:
  ~Output() {}
};

// Finds frames in a byte stream with a fixed buffer.
class FrameDecoder {
 public:
  enum Result { NONE, FRAME, BAD };
  FrameDecoder() : len_(0), overflowed_(false) {}
  // Takes one byte. On FRAME, kind/seq/payload/payloadLen describe it until the next push.
  Result push(uint8_t byte);
  uint8_t kind() const { return buf_[0]; }
  uint8_t seq() const { return buf_[1]; }
  const uint8_t* payload() const { return buf_ + 2; }
  size_t payloadLen() const { return payloadLen_; }

 private:
  uint8_t buf_[MAX_RAW + 1];
  size_t len_;
  size_t payloadLen_;
  bool overflowed_;
};

class Board;

// One part the board hosts. Subclasses fill in the capability's layouts.
class Part {
 public:
  // leaseMs > 0 makes the part an actuator: the board stops it when no command arrives in time.
  Part(const char* name, Capability capability, uint16_t leaseMs, uint16_t readingPeriodMs);

  const char* name() const { return name_; }
  Capability capability() const { return capability_; }
  bool isActuator() const { return leaseMs_ != 0; }
  // True while an actuator runs on a command (not in its safe state).
  bool active() const { return active_; }
  uint8_t id() const { return id_; }

  // Called once when the board starts (set pins up here, not in the constructor).
  virtual void begin() {}
  // Called on every poll(), connected to Faber or not: for parts that must watch their pins all
  // the time (a button's debounce), so the sketch can use them without Faber too.
  virtual void update(uint32_t nowMs) { (void)nowMs; }
  // The versioned capability id PART sends ("motor@1"). Each capability's class gives its own,
  // so a sketch carries only the ids of the capabilities it uses (on an AVR every string literal
  // takes RAM). A part that derives from Part directly returns its own ("planar-base@1").
  virtual const char* capabilityId() const = 0;
  // The capability's info layout. Returns its length.
  virtual uint8_t writeInfo(uint8_t* out) const = 0;
  // The capability's reading layout, or 0 for "nothing to report".
  virtual uint8_t writeReading(uint8_t* out, uint32_t nowMs) = 0;
  // Actuators: apply a command layout. Writes what was really done (a command layout) to `out`
  // and returns its length; 0 refuses the command. `clamped` says a limit changed it.
  virtual uint8_t apply(const uint8_t* command, uint8_t len, uint8_t* out, bool* clamped);
  // Actuators: go to the safe state now. Writes the safe command's layout and returns its length.
  virtual uint8_t enterSafeState(uint8_t* out);
  // The pins it uses, as (role, pin) byte pairs (a PinRole, then the Arduino pin number), at
  // most MAX_PINS of them. Returns the number of bytes written; 0 says nothing, and the board
  // then sends no PINS for it (a part whose pins the library doesn't know).
  virtual uint8_t writePins(uint8_t* out) const;
  static const uint8_t MAX_PINS = 8;

 protected:
  ~Part() {}
  // How often the board asks for a reading; 0 = never (e.g. a motor without an encoder).
  void setReadingPeriod(uint16_t ms) { readingPeriodMs_ = ms; }

 private:
  friend class Board;
  const char* name_;
  Part* next_;
  uint32_t lastCommandMs_;
  uint32_t lastReadingMs_;
  uint16_t leaseMs_;
  uint16_t readingPeriodMs_;
  Capability capability_;
  uint8_t id_;
  bool active_;
  bool added_;
};

enum MotorKind : uint8_t { MOTOR_DC = 0, MOTOR_STEPPER = 1, MOTOR_BLDC = 2, MOTOR_CONTINUOUS_SERVO = 3 };

// motor@1, open loop: set-duty always, set-velocity when a maximum velocity is given (it then
// maps to duty = velocity / max). The safe state is set-velocity 0. It sends no readings; a
// motor with an encoder overrides writeReading() and calls setReadingPeriod().
class Motor : public Part {
 public:
  Motor(const char* name, float maxVelocityRadS = 0, MotorKind kind = MOTOR_DC,
        uint16_t leaseMs = DEFAULT_LEASE_MS);
  // The output now, -1..1.
  float duty() const { return duty_; }
  // Its speed at full duty (rad/s): with it the motor takes set-velocity too. 0 = unknown.
  // Set it before Faber asks for the parts (in setup() is fine).
  void setMaxVelocity(float radS) { maxVelocity_ = radS > 0 && isFinite(radS) ? radS : 0; }

  const char* capabilityId() const override { return "motor@1"; }
  uint8_t writeInfo(uint8_t* out) const override;
  uint8_t writeReading(uint8_t* out, uint32_t nowMs) override;
  uint8_t apply(const uint8_t* command, uint8_t len, uint8_t* out, bool* clamped) override;
  uint8_t enterSafeState(uint8_t* out) override;

 protected:
  ~Motor() {}
  // Drives the hardware at `duty` (-1..1, the sign is the direction).
  virtual void drive(float duty) = 0;
  float maxVelocity_;

 private:
  float duty_;
  MotorKind kind_;
};

enum DistanceKind : uint8_t { DISTANCE_ULTRASONIC = 0, DISTANCE_LASER_TOF = 1, DISTANCE_INFRARED = 2, DISTANCE_RADAR = 3 };

// distance@1. measure() returns metres; a negative or non-finite value means "no target".
class Distance : public Part {
 public:
  Distance(const char* name, DistanceKind kind, float minM, float maxM, float fieldOfViewRad,
           uint16_t readingPeriodMs = 50);

  const char* capabilityId() const override { return "distance@1"; }
  uint8_t writeInfo(uint8_t* out) const override;
  uint8_t writeReading(uint8_t* out, uint32_t nowMs) override;

 protected:
  ~Distance() {}
  virtual float measure() = 0;

 private:
  float minM_, maxM_, fov_;
  DistanceKind kind_;
};

enum BatteryChemistry : uint8_t {
  BATTERY_LI_ION = 0,
  BATTERY_LIPO = 1,
  BATTERY_LIFEPO4 = 2,
  BATTERY_NIMH = 3,
  BATTERY_LEAD_ACID = 4,
};

enum BatteryLevel : uint8_t { BATTERY_OK = 0, BATTERY_LOW = 1, BATTERY_CRITICAL = 2 };

// battery@1 from the pack's voltage. The level comes from typical per-chemistry thresholds per
// cell (the same numbers as faber-capabilities' battery module; the interop tests check that they
// agree), with a little hysteresis so a wobbling voltage doesn't flip it: Li-ion and LiPo are low
// below 3.5 V a cell and critical below 3.3 V (10.5 V and 9.9 V for 3S). It sends the voltage and
// the level only; no current, charge or temperature.
class Battery : public Part {
 public:
  // capacityAh 0 = unknown. cellsInSeries 0 counts as 1.
  Battery(const char* name, BatteryChemistry chemistry, uint8_t cellsInSeries, float capacityAh = 0,
          uint16_t readingPeriodMs = 500);

  const char* capabilityId() const override { return "battery@1"; }
  uint8_t writeInfo(uint8_t* out) const override;
  uint8_t writeReading(uint8_t* out, uint32_t nowMs) override;
  // The level at `volts`, coming from `previous` (the level reported last).
  BatteryLevel levelAt(float volts, BatteryLevel previous) const;
  // The level reported last (BATTERY_OK before the first reading).
  BatteryLevel level() const { return static_cast<BatteryLevel>(level_); }

 protected:
  ~Battery() {}
  // The pack's voltage now (V). A negative or non-finite value means "can't measure": nothing is
  // sent.
  virtual float measureVolts() = 0;

 private:
  float capacityAh_;
  BatteryChemistry chemistry_;
  uint8_t cells_;
  uint8_t level_;
};

// A digital input's level, debounced: a new level counts once the pin has held it for
// debounceMs (a mechanical switch bounces for a few ms when it closes or opens). The first sample
// counts at once.
class Debouncer {
 public:
  explicit Debouncer(uint16_t debounceMs) : sinceMs_(0), ms_(debounceMs), raw_(false), level_(false), started_(false) {}
  // Feeds the raw level at nowMs. True when the debounced level changed (or was first known).
  bool update(bool raw, uint32_t nowMs);
  bool level() const { return level_; }
  bool known() const { return started_; }

 private:
  uint32_t sinceMs_;
  uint16_t ms_;
  bool raw_, level_, started_;
};

// A debounced digital input sent as a one-byte level on every change, and again every second so
// a Faber that connects later learns it. Subclasses give the capability (digital-in@1, obstacle@1).
class DebouncedInput : public Part {
 public:
  // The debounced logical level: true while active (pressed, detected).
  bool level() const { return debouncer_.level(); }
  void update(uint32_t nowMs) override;
  uint8_t writeReading(uint8_t* out, uint32_t nowMs) override;

 protected:
  DebouncedInput(const char* name, Capability capability, uint16_t debounceMs);
  ~DebouncedInput() {}
  // The logical level now, undebounced: true is active (an active-low input inverts its pin).
  virtual bool readLevel() = 0;

 private:
  Debouncer debouncer_;
  uint32_t lastSentMs_;
  bool changed_;
};

// digital-in@1: a button, a switch, an end stop. No info.
class DigitalIn : public DebouncedInput {
 public:
  explicit DigitalIn(const char* name, uint16_t debounceMs = 20) : DebouncedInput(name, DIGITAL_IN, debounceMs) {}
  const char* capabilityId() const override { return "digital-in@1"; }
  uint8_t writeInfo(uint8_t*) const override { return 0; }

 protected:
  ~DigitalIn() {}
};

enum ObstacleKind : uint8_t { OBSTACLE_BUMPER = 0, OBSTACLE_INFRARED = 1, OBSTACLE_LIMIT = 2 };

// obstacle@1: something is in the way (a bumper, a whisker, an infrared obstacle module).
// triggerDistanceM: how close something must come, 0 or less when not known.
class Obstacle : public DebouncedInput {
 public:
  Obstacle(const char* name, ObstacleKind kind, float triggerDistanceM = 0, uint16_t debounceMs = 10)
      : DebouncedInput(name, OBSTACLE, debounceMs), trigger_(triggerDistanceM), kind_(kind) {}
  const char* capabilityId() const override { return "obstacle@1"; }
  uint8_t writeInfo(uint8_t* out) const override;

 protected:
  ~Obstacle() {}

 private:
  float trigger_;
  ObstacleKind kind_;
};

// digital-out@1 (on or off) or pwm-out@1 (a duty 0..1): an LED, a relay, a MOSFET. Leased like a
// motor: no command for its lease time (1 s) and it goes off, as it does on the e-stop and when
// the heartbeat stops. It reports its value 10 times a second. No info.
class PinOutput : public Part {
 public:
  PinOutput(const char* name, bool pwm, uint16_t leaseMs = OUTPUT_LEASE_MS);
  const char* capabilityId() const override { return pwm_ ? "pwm-out@1" : "digital-out@1"; }
  uint8_t writeInfo(uint8_t*) const override { return 0; }
  uint8_t writeReading(uint8_t* out, uint32_t nowMs) override;
  uint8_t apply(const uint8_t* command, uint8_t len, uint8_t* out, bool* clamped) override;
  uint8_t enterSafeState(uint8_t* out) override;
  // The output now: 0 or 1 on/off, the duty 0..1 for PWM (1 = on, whatever the wiring).
  float value() const { return value_; }
  bool isPwm() const { return pwm_; }

 protected:
  ~PinOutput() {}
  // Drives the hardware: 0 (off) or 1 (on), or a duty 0..1.
  virtual void drive(float value) = 0;

 private:
  uint8_t writeCommand(uint8_t* out) const;
  float value_;
  bool pwm_;
};

// What a servo does when its lease ends, on the e-stop and when Faber goes quiet (D29).
enum ServoSafeState : uint8_t {
  // Keep the angle it was last told (the default): an arm holding something doesn't drop it.
  SERVO_HOLD = 0,
  // Stop the pulses, so it goes limp.
  SERVO_RELEASE = 1,
};

// servo@1: a hobby servo turned to an angle (rad), within minRad..maxRad. It starts released (no
// pulses, so it doesn't jump at power-up) and holds an angle once told one. Commands: set-angle
// (clamped to the range), release, and safe (its safe state: hold or release). It reports the
// angle it holds 10 times a second; a servo gives no feedback, so it is the angle it was told.
class ServoPart : public Part {
 public:
  ServoPart(const char* name, float minRad, float maxRad, ServoSafeState safeState = SERVO_HOLD,
            uint16_t leaseMs = OUTPUT_LEASE_MS);
  const char* capabilityId() const override { return "servo@1"; }
  uint8_t writeInfo(uint8_t* out) const override;
  uint8_t writeReading(uint8_t* out, uint32_t nowMs) override;
  uint8_t apply(const uint8_t* command, uint8_t len, uint8_t* out, bool* clamped) override;
  uint8_t enterSafeState(uint8_t* out) override;
  // True while it holds an angle (it gets pulses); false while released.
  bool holding() const { return holding_; }
  // The angle it holds (rad), when it holds one.
  float angle() const { return angle_; }

 protected:
  ~ServoPart() {}
  // Turns the servo to `rad` (within the range), starting its pulses if they are off.
  virtual void moveTo(float rad) = 0;
  // Stops its pulses.
  virtual void letGo() = 0;
  float minRad_, maxRad_;

 private:
  uint8_t release(uint8_t* out);
  float angle_;
  ServoSafeState safeState_;
  bool holding_;
};

// The board's side of the protocol.
class Board {
 public:
  explicit Board(Output& out);

  void setIdentity(const char* board, const char* firmware, const char* firmwareVersion);
  // The build id Faber's Arduino editor gives a sketch (FABER_BUILD_ID). It travels at the end of
  // HELLO_ACK, so Faber can tell which code the board runs; without it the HELLO_ACK is exactly
  // what it always was.
  // Kept as its wire bytes, so a sketch without one pays almost nothing for it.
  void setBuildId(uint64_t id) {
    for (uint8_t i = 0; i < 8; i++) buildId_[i] = static_cast<uint8_t>(id >> (8 * i));
    hasBuildId_ = true;
  }
  // Clamped to 100..1000 ms.
  void setHeartbeatTimeout(uint16_t ms);
  // The pin number of A0 on this board (14 on an Uno, 54 on a Mega), so Faber can name pins
  // from it on A0, A1, ...; NO_PIN (the default) when it isn't known.
  void setAnalogBase(uint8_t pin) { analogBase_ = pin; }
  // For tests: answer HELLO as if this library spoke only versions lo..hi.
  void speakVersions(uint8_t lo, uint8_t hi) {
    minVersion_ = lo;
    maxVersion_ = hi;
  }
  uint16_t heartbeatTimeout() const { return heartbeatTimeoutMs_; }
  // Adds a part; its id is the order it was added in. False once 254 parts are added, or if the
  // part already belongs to a board.
  bool add(Part& part);
  // Calls begin() on every part.
  void begin();

  // Feed every received byte, and call tick() often (every loop()).
  void feed(uint8_t byte, uint32_t nowMs);
  void tick(uint32_t nowMs);

  // A hardware stop button: latches until resetHardwareEstop() AND a release from Faber.
  void engageHardwareEstop();
  void resetHardwareEstop();
  void log(LogLevel level, const char* text);

  bool connected() const { return version_ != 0; }
  bool heartbeatAlive(uint32_t nowMs) const;
  bool estopEngaged() const { return hwLatched_ || releasePending_ || coreEstop_; }
  bool failsafe() const { return failsafe_; }
  uint16_t rxBad() const { return rxBad_; }
  uint16_t rxGaps() const { return rxGaps_; }
  uint8_t partCount() const { return count_; }

 private:
  void handle(uint8_t kind, const uint8_t* p, size_t len, uint32_t nowMs);
  void onHello(const uint8_t* p, size_t len, uint32_t nowMs);
  void onCommand(const uint8_t* p, size_t len, uint32_t nowMs);
  void checkSafety(uint32_t nowMs);
  void safeAll(Cause cause);
  void safe(Part& part, Cause cause);
  void sendApplied(Part& part, Cause cause, bool clamped, const uint8_t* cmd, uint8_t len);
  void sendStatus(uint32_t nowMs);
  void sendPart(Part& part);
  void sendPins(const Part& part);
  void sendEstop(bool engaged);
  Part* find(uint8_t id);
  // Builds a frame in tx_ and writes it COBS-encoded.
  void send(uint8_t kind, size_t payloadLen);
  uint8_t* payload() { return tx_ + 2; }
  size_t maxPayload() const { return txMaxPayload_; }
  uint8_t putStr(uint8_t* at, const char* s, size_t room);

  Output& out_;
  FrameDecoder decoder_;
  uint8_t tx_[MAX_RAW];
  Part* first_;
  Part* last_;
  const char* board_;
  const char* firmware_;
  const char* firmwareVersion_;
  uint8_t buildId_[8];
  uint32_t lastHeartbeatMs_;
  uint32_t lastStatusMs_;
  uint16_t heartbeatTimeoutMs_;
  uint16_t rxBad_;
  uint16_t rxGaps_;
  uint8_t txMaxPayload_;
  uint8_t count_;
  uint8_t version_;
  uint8_t txSeq_;
  uint8_t rxSeq_;
  uint8_t lastFault_;
  uint8_t minVersion_;
  uint8_t maxVersion_;
  uint8_t analogBase_;
  bool rxSeqKnown_;
  bool haveHeartbeat_;
  bool failsafe_;
  bool coreEstop_;
  bool hwLatched_;
  bool releasePending_;
  bool started_;
  bool hasBuildId_;
};

}  // namespace faberlink

#endif
