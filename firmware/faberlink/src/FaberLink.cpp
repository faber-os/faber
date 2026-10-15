// Arduino glue for FaberLink: the serial port, millis(), and the built-in part types.

#include "FaberLink.h"

// FaberMotor and FaberDistance declared as globals register themselves here, and
// FaberLink::begin() adds them to the board in declaration order. A fixed table: no allocation.
namespace {
const uint8_t MAX_AUTO_PARTS = 16;
faberlink::Part* pending[MAX_AUTO_PARTS];
uint8_t pendingCount = 0;

void enqueue(faberlink::Part* p) {
  if (pendingCount < MAX_AUTO_PARTS) pending[pendingCount++] = p;
}
}  // namespace

FaberMotor::FaberMotor(const char* name, uint8_t pwmPin, uint8_t directionPin, float maxVelocityRadS)
    : faberlink::Motor(name, maxVelocityRadS), pwmPin_(pwmPin), directionPin_(directionPin) {
  enqueue(this);
}

void FaberMotor::begin() {
  pinMode(pwmPin_, OUTPUT);
  pinMode(directionPin_, OUTPUT);
  analogWrite(pwmPin_, 0);
}

void FaberMotor::drive(float duty) {
  digitalWrite(directionPin_, duty < 0 ? HIGH : LOW);
  float magnitude = duty < 0 ? -duty : duty;
  analogWrite(pwmPin_, static_cast<int>(magnitude * 255.0f + 0.5f));
}

FaberDistance::FaberDistance(const char* name, uint8_t analogPin, float minM, float maxM,
                             faberlink::DistanceKind kind, ToMeters toMeters)
    : faberlink::Distance(name, kind, minM, maxM, 0.1f), toMeters_(toMeters), maxM_(maxM), pin_(analogPin) {
  enqueue(this);
}

float FaberDistance::measure() {
  int raw = analogRead(pin_);
  if (toMeters_) return toMeters_(raw);
  return raw * maxM_ / 1023.0f;
}

FaberLink::FaberLink(HardwareSerial& port) : port_(port), serial_(&port), output_(port), board_(output_) {}

FaberLink::FaberLink(Stream& port) : port_(port), serial_(0), output_(port), board_(output_) {}

void FaberLink::begin(unsigned long baud, const char* board, const char* firmwareVersion,
                      uint16_t heartbeatTimeoutMs) {
  if (serial_) serial_->begin(baud);
  board_.setIdentity(board, "faberlink-arduino", firmwareVersion);
  board_.setHeartbeatTimeout(heartbeatTimeoutMs);
  for (uint8_t i = 0; i < pendingCount; i++) board_.add(*pending[i]);
  pendingCount = 0;
  board_.begin();
}

bool FaberLink::add(faberlink::Part& part) { return board_.add(part); }

void FaberLink::poll() {
  while (port_.available() > 0) {
    board_.feed(static_cast<uint8_t>(port_.read()), millis());
  }
  board_.tick(millis());
}

void FaberLink::engageEstop() { board_.engageHardwareEstop(); }

void FaberLink::resetEstop() { board_.resetHardwareEstop(); }

void FaberLink::log(const char* text) { board_.log(faberlink::LOG_INFO, text); }
