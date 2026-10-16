// FaberLink protocol core. See FaberLinkCore.h and docs/faberlink.md.

#include "FaberLinkCore.h"

#include <string.h>

namespace faberlink {

// ---------------------------------------------------------------------------------------------
// Bytes

uint16_t crc16(const uint8_t* data, size_t len, uint16_t crc) {
  for (size_t i = 0; i < len; i++) {
    crc ^= static_cast<uint16_t>(data[i]) << 8;
    for (uint8_t bit = 0; bit < 8; bit++) {
      crc = (crc & 0x8000) ? static_cast<uint16_t>((crc << 1) ^ 0x1021) : static_cast<uint16_t>(crc << 1);
    }
  }
  return crc;
}

int cobsDecode(uint8_t* buf, size_t len) {
  size_t r = 0, w = 0;
  while (r < len) {
    uint8_t code = buf[r];
    if (code == 0) return -1;
    r++;
    size_t end = r + code - 1;
    if (end > len) return -1;
    while (r < end) {
      if (buf[r] == 0) return -1;
      buf[w++] = buf[r++];
    }
    if (code != 0xFF && r < len) buf[w++] = 0;
  }
  return static_cast<int>(w);
}

bool isFinite(float f) {
  uint32_t u;
  memcpy(&u, &f, 4);
  return (u & 0x7F800000UL) != 0x7F800000UL;
}

void putU16(uint8_t* p, uint16_t v) {
  p[0] = static_cast<uint8_t>(v);
  p[1] = static_cast<uint8_t>(v >> 8);
}

void putU32(uint8_t* p, uint32_t v) {
  for (uint8_t i = 0; i < 4; i++) p[i] = static_cast<uint8_t>(v >> (8 * i));
}

void putF32(uint8_t* p, float v) {
  uint32_t u;
  memcpy(&u, &v, 4);
  putU32(p, u);
}

uint16_t getU16(const uint8_t* p) { return static_cast<uint16_t>(p[0] | (static_cast<uint16_t>(p[1]) << 8)); }

uint32_t getU32(const uint8_t* p) {
  uint32_t v = 0;
  for (uint8_t i = 0; i < 4; i++) v |= static_cast<uint32_t>(p[i]) << (8 * i);
  return v;
}

float getF32(const uint8_t* p) {
  uint32_t u = getU32(p);
  float f;
  memcpy(&f, &u, 4);
  return f;
}

static float clampf(float v, float lo, float hi, bool* clamped) {
  if (v < lo) {
    *clamped = true;
    return lo;
  }
  if (v > hi) {
    *clamped = true;
    return hi;
  }
  return v;
}

// ---------------------------------------------------------------------------------------------
// Frames

FrameDecoder::Result FrameDecoder::push(uint8_t byte) {
  if (byte != 0) {
    if (len_ < sizeof(buf_)) {
      buf_[len_++] = byte;
    } else {
      overflowed_ = true;
    }
    return NONE;
  }
  size_t len = len_;
  len_ = 0;
  if (overflowed_) {
    overflowed_ = false;
    return BAD;
  }
  if (len == 0) return NONE;  // an empty frame: a flush
  int n = cobsDecode(buf_, len);
  if (n < 4 || static_cast<size_t>(n) > MAX_RAW) return BAD;
  size_t body = static_cast<size_t>(n) - 2;
  if (crc16(buf_, body) != getU16(buf_ + body)) return BAD;
  payloadLen_ = body - 2;
  return FRAME;
}

// ---------------------------------------------------------------------------------------------
// Parts

static const char* capabilityId(Capability c) {
  switch (c) {
    case DISTANCE:
      return "distance@1";
    case MOTOR:
      return "motor@1";
    case PLANAR_BASE:
      return "planar-base@1";
    case ESTOP_CAP:
      return "estop@1";
    case BATTERY:
      return "battery@1";
  }
  return "";
}

Part::Part(const char* name, Capability capability, uint16_t leaseMs, uint16_t readingPeriodMs)
    : name_(name),
      next_(0),
      lastCommandMs_(0),
      lastReadingMs_(0),
      leaseMs_(leaseMs),
      readingPeriodMs_(readingPeriodMs),
      capability_(capability),
      id_(0),
      active_(false),
      added_(false) {}

uint8_t Part::apply(const uint8_t*, uint8_t, uint8_t*, bool*) { return 0; }

uint8_t Part::enterSafeState(uint8_t*) { return 0; }

uint8_t Part::writePins(uint8_t*) const { return 0; }

// motor@1 command layout: kind (0 set-velocity, 1 set-position, 2 set-duty), value f32.
Motor::Motor(const char* name, float maxVelocityRadS, MotorKind kind, uint16_t leaseMs)
    : Part(name, MOTOR, leaseMs == 0 ? DEFAULT_LEASE_MS : leaseMs, 0),
      maxVelocity_(maxVelocityRadS > 0 && isFinite(maxVelocityRadS) ? maxVelocityRadS : 0),
      duty_(0),
      kind_(kind) {}

uint8_t Motor::writeInfo(uint8_t* out) const {
  out[0] = kind_;
  out[1] = static_cast<uint8_t>((maxVelocity_ > 0 ? 1 : 0) | 4);  // velocity (if known), duty
  if (maxVelocity_ > 0) {
    putF32(out + 2, maxVelocity_);
  } else {
    putU32(out + 2, 0x7FC00000UL);  // NaN: no maximum known
  }
  return 6;
}

uint8_t Motor::writeReading(uint8_t*, uint32_t) { return 0; }

uint8_t Motor::apply(const uint8_t* c, uint8_t len, uint8_t* out, bool* clamped) {
  if (len != 5) return 0;
  uint8_t k = c[0];
  float v = getF32(c + 1);
  if (!isFinite(v)) return 0;
  float duty;
  if (k == 0 && maxVelocity_ > 0) {
    v = clampf(v, -maxVelocity_, maxVelocity_, clamped);
    duty = v / maxVelocity_;
  } else if (k == 2) {
    v = clampf(v, -1, 1, clamped);
    duty = v;
  } else {
    return 0;  // set-position, or set-velocity without a known maximum
  }
  duty_ = duty;
  drive(duty);
  out[0] = k;
  putF32(out + 1, v);
  return 5;
}

uint8_t Motor::enterSafeState(uint8_t* out) {
  duty_ = 0;
  drive(0);
  out[0] = 0;  // set-velocity 0
  putF32(out + 1, 0);
  return 5;
}

Distance::Distance(const char* name, DistanceKind kind, float minM, float maxM, float fieldOfViewRad,
                   uint16_t readingPeriodMs)
    : Part(name, DISTANCE, 0, readingPeriodMs), minM_(minM), maxM_(maxM), fov_(fieldOfViewRad), kind_(kind) {}

uint8_t Distance::writeInfo(uint8_t* out) const {
  putF32(out, minM_);
  putF32(out + 4, maxM_);
  putF32(out + 8, fov_);
  out[12] = kind_;
  return 13;
}

// distance@1 reading layout: metres f32, status (0 ok, 1 too-close, 2 too-far, 3 no-target).
uint8_t Distance::writeReading(uint8_t* out, uint32_t) {
  float m = measure();
  uint8_t status = 0;
  if (!isFinite(m) || m < 0) {
    m = maxM_;
    status = 3;
  } else if (m < minM_) {
    m = minM_;
    status = 1;
  } else if (m > maxM_) {
    m = maxM_;
    status = 2;
  }
  putF32(out, m);
  out[4] = status;
  return 5;
}

// ---------------------------------------------------------------------------------------------
// Battery

namespace {

// One cell's thresholds (V): typical datasheet values, as in faber-capabilities' battery.rs.
// A switch, not a table, so they stay in flash on an AVR instead of taking RAM.
void cellThresholds(BatteryChemistry c, float* low, float* critical, float* hysteresis) {
  switch (c) {
    case BATTERY_LIFEPO4:
      *low = 3.0f;
      *critical = 2.8f;
      *hysteresis = 0.03f;
      return;
    case BATTERY_NIMH:
      *low = 1.1f;
      *critical = 1.0f;
      *hysteresis = 0.02f;
      return;
    case BATTERY_LEAD_ACID:
      *low = 11.8f / 6.0f;
      *critical = 11.5f / 6.0f;
      *hysteresis = 0.02f;
      return;
    case BATTERY_LI_ION:
    case BATTERY_LIPO:
    default:
      *low = 3.5f;
      *critical = 3.3f;
      *hysteresis = 0.05f;
      return;
  }
}

}  // namespace

Battery::Battery(const char* name, BatteryChemistry chemistry, uint8_t cellsInSeries, float capacityAh,
                 uint16_t readingPeriodMs)
    : Part(name, BATTERY, 0, readingPeriodMs),
      capacityAh_(capacityAh),
      chemistry_(chemistry),
      cells_(cellsInSeries == 0 ? 1 : cellsInSeries),
      level_(BATTERY_OK) {}

// battery@1 info layout: chemistry u8, cells-in-series u8, capacity-ah f32 (NaN = unknown).
uint8_t Battery::writeInfo(uint8_t* out) const {
  out[0] = chemistry_;
  out[1] = cells_;
  if (capacityAh_ > 0) {
    putF32(out + 2, capacityAh_);
  } else {
    putU32(out + 2, 0x7FC00000UL);  // a quiet NaN
  }
  return 6;
}

// A level gets worse as soon as the voltage drops below a threshold, and better only once it is
// a hysteresis margin above it.
BatteryLevel Battery::levelAt(float volts, BatteryLevel previous) const {
  float low, critical, hysteresis;
  cellThresholds(chemistry_, &low, &critical, &hysteresis);
  const float n = cells_;
  BatteryLevel raw = !(volts >= critical * n) ? BATTERY_CRITICAL : volts < low * n ? BATTERY_LOW : BATTERY_OK;
  if (raw >= previous) return raw;
  float m = hysteresis;
  BatteryLevel margin =
      !(volts >= (critical + m) * n) ? BATTERY_CRITICAL : volts < (low + m) * n ? BATTERY_LOW : BATTERY_OK;
  return margin < previous ? margin : previous;
}

// battery@1 reading layout: present u8 (nothing optional here: 0), volts f32, amps f32,
// charge f32, celsius f32 (all 0), level u8, cell count u8 (0): 19 bytes.
uint8_t Battery::writeReading(uint8_t* out, uint32_t) {
  float v = measureVolts();
  if (!isFinite(v) || v < 0) return 0;
  level_ = levelAt(v, static_cast<BatteryLevel>(level_));
  out[0] = 0;
  putF32(out + 1, v);
  putF32(out + 5, 0);
  putF32(out + 9, 0);
  putF32(out + 13, 0);
  out[17] = level_;
  out[18] = 0;
  return 19;
}

// ---------------------------------------------------------------------------------------------
// Board

Board::Board(Output& out)
    : out_(out),
      first_(0),
      last_(0),
      board_("board"),
      firmware_("faberlink"),
      firmwareVersion_("0.1.0"),
      lastHeartbeatMs_(0),
      lastStatusMs_(0),
      heartbeatTimeoutMs_(DEFAULT_HEARTBEAT_TIMEOUT_MS),
      rxBad_(0),
      rxGaps_(0),
      txMaxPayload_(FABERLINK_MAX_PAYLOAD),
      count_(0),
      version_(0),
      txSeq_(0),
      rxSeq_(0),
      lastFault_(FAULT_NONE),
      minVersion_(MIN_VERSION),
      maxVersion_(MAX_VERSION),
      analogBase_(NO_PIN),
      rxSeqKnown_(false),
      haveHeartbeat_(false),
      failsafe_(false),
      coreEstop_(false),
      hwLatched_(false),
      releasePending_(false),
      started_(false),
      hasBuildId_(false) {}

void Board::setIdentity(const char* board, const char* firmware, const char* firmwareVersion) {
  board_ = board;
  firmware_ = firmware;
  firmwareVersion_ = firmwareVersion;
}

void Board::setHeartbeatTimeout(uint16_t ms) {
  if (ms < MIN_HEARTBEAT_TIMEOUT_MS) ms = MIN_HEARTBEAT_TIMEOUT_MS;
  if (ms > MAX_HEARTBEAT_TIMEOUT_MS) ms = MAX_HEARTBEAT_TIMEOUT_MS;
  heartbeatTimeoutMs_ = ms;
}

bool Board::add(Part& part) {
  if (part.added_ || count_ >= 254) return false;
  part.added_ = true;
  part.id_ = count_++;
  part.next_ = 0;
  if (last_) {
    last_->next_ = &part;
  } else {
    first_ = &part;
  }
  last_ = &part;
  return true;
}

void Board::begin() {
  if (started_) return;
  started_ = true;
  for (Part* p = first_; p; p = p->next_) {
    p->begin();
    uint8_t buf[16];
    if (p->isActuator()) p->enterSafeState(buf);  // things that move start stopped
  }
}

Part* Board::find(uint8_t id) {
  for (Part* p = first_; p; p = p->next_) {
    if (p->id_ == id) return p;
  }
  return 0;
}

bool Board::heartbeatAlive(uint32_t nowMs) const {
  return haveHeartbeat_ && static_cast<uint32_t>(nowMs - lastHeartbeatMs_) < heartbeatTimeoutMs_;
}

void Board::feed(uint8_t byte, uint32_t nowMs) {
  FrameDecoder::Result r = decoder_.push(byte);
  if (r == FrameDecoder::BAD) {
    if (rxBad_ != 0xFFFF) rxBad_++;
    lastFault_ = FAULT_BAD_FRAMES;
    return;
  }
  if (r != FrameDecoder::FRAME) return;
  // The safety rules run before every message, so a late heartbeat can't hide a timeout.
  checkSafety(nowMs);
  uint8_t seq = decoder_.seq();
  if (rxSeqKnown_ && seq != rxSeq_) {
    uint16_t gaps = rxGaps_ + static_cast<uint8_t>(seq - rxSeq_);
    rxGaps_ = gaps < rxGaps_ ? 0xFFFF : gaps;
  }
  rxSeq_ = static_cast<uint8_t>(seq + 1);
  rxSeqKnown_ = true;
  handle(decoder_.kind(), decoder_.payload(), decoder_.payloadLen(), nowMs);
}

void Board::handle(uint8_t k, const uint8_t* p, size_t len, uint32_t nowMs) {
  if (k == kind::HELLO) {
    onHello(p, len, nowMs);
    return;
  }
  if (!connected()) return;
  switch (k) {
    case kind::DESCRIBE:
      // Each part's pins just before it, so Faber has them when the last PART completes the list.
      for (Part* part = first_; part; part = part->next_) {
        sendPins(*part);
        sendPart(*part);
      }
      break;
    case kind::HEARTBEAT:
      if (len != 1 || (p[0] & 0xFE)) return;
      lastHeartbeatMs_ = nowMs;
      haveHeartbeat_ = true;
      failsafe_ = false;
      if ((p[0] & 1) && !coreEstop_) {
        coreEstop_ = true;
        lastFault_ = FAULT_ESTOP;
        safeAll(CAUSE_ESTOP);
      } else if (!(p[0] & 1)) {
        coreEstop_ = false;  // a level: never clears a hardware latch (releasePending_)
      }
      break;
    case kind::COMMAND:
      onCommand(p, len, nowMs);
      break;
    case kind::SAFE:
      if (len != 1) return;
      if (p[0] == 0xFF) {
        for (Part* part = first_; part; part = part->next_) {
          if (part->isActuator()) safe(*part, CAUSE_SAFE);
        }
      } else {
        Part* part = find(p[0]);
        if (part && part->isActuator()) safe(*part, CAUSE_SAFE);
      }
      break;
    case kind::ESTOP:
      if (len < 3 || p[0] > 1) return;
      if (p[0]) {
        coreEstop_ = true;
        lastFault_ = FAULT_ESTOP;
        safeAll(CAUSE_ESTOP);
      } else {
        coreEstop_ = false;
        // Released in Faber. It only counts for a hardware stop once that has been reset here.
        if (!hwLatched_) releasePending_ = false;
      }
      break;
    default:
      break;
  }
}

void Board::onHello(const uint8_t* p, size_t len, uint32_t nowMs) {
  if (len != 3) return;
  uint8_t lo = p[0] > minVersion_ ? p[0] : minVersion_;
  uint8_t hi = p[1] < maxVersion_ ? p[1] : maxVersion_;
  // A new session: whatever ran for the last one stops, silently, until it is commanded again.
  for (Part* part = first_; part; part = part->next_) {
    uint8_t buf[16];
    if (part->isActuator() && part->active_) {
      part->enterSafeState(buf);
      part->active_ = false;
    }
  }
  version_ = lo <= hi ? hi : 0;
  haveHeartbeat_ = false;
  failsafe_ = false;
  // The HELLO_ACK itself always fits our own buffers; later frames also fit the core's.
  txMaxPayload_ = FABERLINK_MAX_PAYLOAD;
  uint8_t* o = payload();
  o[0] = version_;
  o[1] = minVersion_;
  o[2] = maxVersion_;
  o[3] = FABERLINK_MAX_PAYLOAD;
  putU16(o + 4, heartbeatTimeoutMs_);
  o[6] = count_;
  size_t n = 7;
  // The strings share what is left, after room for the optional build id at the end. (Both
  // shares of the first one are constants: a division at run time costs an AVR ~100 bytes.)
  const size_t room = FABERLINK_MAX_PAYLOAD - (hasBuildId_ ? 8 : 0);
  n += putStr(o + n, board_, hasBuildId_ ? (FABERLINK_MAX_PAYLOAD - 15) / 3 : (FABERLINK_MAX_PAYLOAD - 7) / 3);
  n += putStr(o + n, firmware_, (room - n) / 2);
  n += putStr(o + n, firmwareVersion_, room - n);
  if (hasBuildId_) {
    memcpy(o + n, buildId_, 8);
    n += 8;
  }
  send(kind::HELLO_ACK, n);
  if (p[2] >= 32 && p[2] < txMaxPayload_) txMaxPayload_ = p[2];
  lastStatusMs_ = nowMs - STATUS_PERIOD_MS;  // a STATUS on the next tick
}

void Board::onCommand(const uint8_t* p, size_t len, uint32_t nowMs) {
  if (len < 2) return;
  Part* part = find(p[0]);
  if (!part || !part->isActuator()) return;
  if (p[1] != part->capability()) {
    lastFault_ = FAULT_REFUSED_COMMAND;
    safe(*part, CAUSE_REFUSED);
    return;
  }
  if (estopEngaged()) {
    safe(*part, CAUSE_ESTOP);
    return;
  }
  if (!heartbeatAlive(nowMs)) {
    safe(*part, CAUSE_NO_HEARTBEAT);
    return;
  }
  uint8_t done[16];
  bool clamped = false;
  uint8_t n = part->apply(p + 2, static_cast<uint8_t>(len - 2), done, &clamped);
  if (n == 0) {
    lastFault_ = FAULT_REFUSED_COMMAND;
    safe(*part, CAUSE_REFUSED);
    return;
  }
  part->active_ = true;
  part->lastCommandMs_ = nowMs;
  sendApplied(*part, CAUSE_COMMAND, clamped, done, n);
}

void Board::checkSafety(uint32_t nowMs) {
  if (haveHeartbeat_ && !failsafe_ && !heartbeatAlive(nowMs)) {
    failsafe_ = true;
    lastFault_ = FAULT_HEARTBEAT_LOST;
    safeAll(CAUSE_HEARTBEAT_LOST);
  }
  if (estopEngaged()) safeAll(CAUSE_ESTOP);
  for (Part* part = first_; part; part = part->next_) {
    if (part->active_ && static_cast<uint32_t>(nowMs - part->lastCommandMs_) >= part->leaseMs_) {
      lastFault_ = FAULT_LEASE_EXPIRED;
      safe(*part, CAUSE_LEASE_EXPIRED);
    }
  }
}

void Board::safeAll(Cause cause) {
  for (Part* part = first_; part; part = part->next_) {
    if (part->active_) safe(*part, cause);
  }
}

void Board::safe(Part& part, Cause cause) {
  uint8_t buf[16];
  uint8_t n = part.enterSafeState(buf);
  part.active_ = false;
  if (connected()) sendApplied(part, cause, false, buf, n);
}

void Board::sendApplied(Part& part, Cause cause, bool clamped, const uint8_t* cmd, uint8_t len) {
  if (static_cast<size_t>(len) + 4 > maxPayload()) return;
  uint8_t* o = payload();
  o[0] = part.id_;
  o[1] = part.capability_;
  o[2] = cause;
  o[3] = clamped ? 1 : 0;
  memcpy(o + 4, cmd, len);
  send(kind::APPLIED, 4 + len);
}

void Board::sendPart(Part& part) {
  uint8_t* o = payload();
  o[0] = part.id_;
  o[1] = count_;
  putU16(o + 2, part.leaseMs_);
  size_t n = 4;
  n += putStr(o + n, capabilityId(part.capability_), 255);
  uint8_t info[32];
  uint8_t infoLen = part.writeInfo(info);
  size_t room = maxPayload() - n - infoLen;  // the name is cut, never the info
  n += putStr(o + n, part.name_, room);
  memcpy(o + n, info, infoLen);
  send(kind::PART, n + infoLen);
}

// PINS: part u8, analog base u8 (NO_PIN = unknown), then (role u8, pin u8) per pin.
void Board::sendPins(const Part& part) {
  uint8_t pins[2 * Part::MAX_PINS];
  uint8_t n = part.writePins(pins);
  if (n == 0 || n > sizeof(pins) || n % 2 != 0) return;
  uint8_t* o = payload();
  o[0] = part.id_;
  o[1] = analogBase_;
  memcpy(o + 2, pins, n);
  send(kind::PINS, 2 + n);
}

void Board::sendStatus(uint32_t nowMs) {
  uint8_t* o = payload();
  bool degraded = failsafe_ || estopEngaged() || rxBad_ > 0;
  o[0] = degraded ? 1 : 0;
  o[1] = static_cast<uint8_t>((hwLatched_ ? 1 : 0) | (estopEngaged() ? 2 : 0) | (failsafe_ ? 4 : 0));
  putU32(o + 2, nowMs);  // the board's clock is its uptime
  o[6] = lastFault_;
  putU16(o + 7, rxBad_);
  putU16(o + 9, rxGaps_);
  send(kind::STATUS, 11);
}

void Board::sendEstop(bool engaged) {
  if (!connected()) return;
  uint8_t* o = payload();
  o[0] = engaged ? 1 : 0;
  o[1] = 0;  // source: part
  size_t n = 2;
  n += putStr(o + n, "stop button", maxPayload() - n);
  send(kind::ESTOP, n);
}

void Board::tick(uint32_t nowMs) {
  checkSafety(nowMs);
  if (!connected()) return;
  if (static_cast<uint32_t>(nowMs - lastStatusMs_) >= STATUS_PERIOD_MS) {
    lastStatusMs_ = nowMs;
    sendStatus(nowMs);
  }
  for (Part* part = first_; part; part = part->next_) {
    if (part->readingPeriodMs_ == 0) continue;
    if (static_cast<uint32_t>(nowMs - part->lastReadingMs_) < part->readingPeriodMs_) continue;
    part->lastReadingMs_ = nowMs;
    uint8_t* o = payload();
    uint8_t n = part->writeReading(o + 6, nowMs);
    if (n == 0) continue;
    o[0] = part->id_;
    o[1] = part->capability_;
    putU32(o + 2, nowMs);
    send(kind::READING, 6 + n);
  }
}

void Board::engageHardwareEstop() {
  bool was = hwLatched_;
  hwLatched_ = true;
  releasePending_ = true;
  lastFault_ = FAULT_ESTOP;
  safeAll(CAUSE_ESTOP);
  if (!was) sendEstop(true);
}

void Board::resetHardwareEstop() { hwLatched_ = false; }

void Board::log(LogLevel level, const char* text) {
  if (!connected()) return;
  uint8_t* o = payload();
  o[0] = level;
  size_t n = 1;
  for (; text[n - 1] && n < maxPayload(); n++) o[n] = static_cast<uint8_t>(text[n - 1]);
  send(kind::LOG, n);
}

uint8_t Board::putStr(uint8_t* at, const char* s, size_t room) {
  size_t len = strlen(s);
  if (room > 0) room--;  // the length byte
  if (len > room) len = room;
  if (len > 255) len = 255;
  at[0] = static_cast<uint8_t>(len);
  memcpy(at + 1, s, len);
  return static_cast<uint8_t>(len + 1);
}

void Board::send(uint8_t k, size_t payloadLen) {
  tx_[0] = k;
  tx_[1] = txSeq_++;
  size_t n = 2 + payloadLen;
  putU16(tx_ + n, crc16(tx_, n));
  n += 2;
  // COBS, written group by group straight from tx_.
  size_t pos = 0;
  for (;;) {
    size_t run = 0;
    while (pos + run < n && tx_[pos + run] != 0 && run < 254) run++;
    uint8_t code = static_cast<uint8_t>(run + 1);
    out_.write(&code, 1);
    out_.write(tx_ + pos, run);
    pos += run;
    if (pos == n) break;
    if (run == 254) continue;  // a full group: no zero to skip
    pos++;                     // the zero this group stands for
    if (pos == n) {
      uint8_t last = 1;  // the data ended in a zero
      out_.write(&last, 1);
      break;
    }
  }
  uint8_t zero = 0;
  out_.write(&zero, 1);
}

}  // namespace faberlink
