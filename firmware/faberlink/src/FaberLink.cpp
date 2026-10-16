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

// Metres per microsecond of echo: sound goes there and back at 343 m/s.
static const float METERS_PER_ECHO_US = 343.0f / 2.0f / 1000000.0f;

FaberUltrasonic::FaberUltrasonic(const char* name, uint8_t triggerPin, uint8_t echoPin, float minM, float maxM)
    // 0.26 rad: the HC-SR04's 15 degree cone. 60 ms between measurements, as its datasheet asks.
    : faberlink::Distance(name, faberlink::DISTANCE_ULTRASONIC, minM, maxM, 0.26f, 60),
      // Wait 10 % longer than an echo from maxM takes, then call it "no target".
      timeoutUs_(static_cast<unsigned long>(maxM / METERS_PER_ECHO_US * 1.1f)),
      triggerPin_(triggerPin),
      echoPin_(echoPin) {
  enqueue(this);
}

void FaberUltrasonic::begin() {
  pinMode(triggerPin_, OUTPUT);
  digitalWrite(triggerPin_, LOW);
  pinMode(echoPin_, INPUT);
}

float FaberUltrasonic::measure() {
  // A 10 us pulse on the trigger starts one measurement.
  digitalWrite(triggerPin_, LOW);
  delayMicroseconds(2);
  digitalWrite(triggerPin_, HIGH);
  delayMicroseconds(10);
  digitalWrite(triggerPin_, LOW);
  unsigned long echoUs = pulseIn(echoPin_, HIGH, timeoutUs_);
  if (echoUs == 0) return -1;  // no echo in time: nothing in range
  return echoUs * METERS_PER_ECHO_US;
}

FaberBattery::FaberBattery(const char* name, uint8_t analogPin, float dividerRatio, uint8_t cellsInSeries,
                           faberlink::BatteryChemistry chemistry, float referenceVolts, float capacityAh)
    : faberlink::Battery(name, chemistry, cellsInSeries, capacityAh),
      voltsPerCount_(referenceVolts * dividerRatio / 1023.0f),
      pin_(analogPin) {
  enqueue(this);
}

// The average of a few reads: a battery's voltage is noisy while motors run. Under a millisecond
// twice a second, so loop() stays fast.
float FaberBattery::measureVolts() {
  const uint8_t reads = 8;
  long sum = 0;
  for (uint8_t i = 0; i < reads; i++) sum += analogRead(pin_);
  return sum * voltsPerCount_ / reads;
}

FaberLink::FaberLink(HardwareSerial& port) : port_(port), serial_(&port), output_(port), board_(output_) {}

FaberLink::FaberLink(Stream& port) : port_(port), serial_(0), output_(port), board_(output_) {}

void FaberLink::begin(unsigned long baud, const char* board, const char* firmwareVersion,
                      uint16_t heartbeatTimeoutMs) {
  if (serial_) serial_->begin(baud);
  board_.setIdentity(board, "faberlink-arduino", firmwareVersion);
#ifdef FABER_BUILD_ID
  // Set by Faber's Arduino editor when it compiles the sketch (docs/arduino-editor.md), so Faber
  // can tell which code runs on the board. The Arduino IDE doesn't set it.
  board_.setBuildId(static_cast<uint64_t>(FABER_BUILD_ID));
#endif
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
