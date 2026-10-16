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

// One pin in PINS: (role, pin). Returns the bytes written: none for a pin that isn't wired.
uint8_t putPin(uint8_t* out, uint8_t role, uint8_t pin) {
  if (pin == faberlink::NO_PIN) return 0;
  out[0] = role;
  out[1] = pin;
  return 2;
}

void output(uint8_t pin) {
  if (pin != faberlink::NO_PIN) pinMode(pin, OUTPUT);
}

// An input with its pull-up on for an active-low part (a switch to GND, an open-collector output).
void input(uint8_t pin, faberlink::ActiveLevel active) {
  pinMode(pin, active == faberlink::ACTIVE_LOW ? INPUT_PULLUP : INPUT);
}

bool isActive(uint8_t pin, faberlink::ActiveLevel level) {
  return (digitalRead(pin) == HIGH) == (level == faberlink::ACTIVE_HIGH);
}
}  // namespace

FaberMotor::FaberMotor(const char* name, uint8_t pwmPin, uint8_t directionPin, float maxVelocityRadS)
    : faberlink::Motor(name, maxVelocityRadS),
      a_(pwmPin),
      b_(directionPin),
      c_(faberlink::NO_PIN),
      standby_(faberlink::NO_PIN),
      driver_(faberlink::DRIVER_PWM_DIR) {
  enqueue(this);
}

FaberMotor::FaberMotor(const char* name, faberlink::MotorDriver driver, uint8_t pin1, uint8_t pin2,
                       uint8_t pin3, uint8_t pin4)
    : faberlink::Motor(name),
      a_(pin1),
      b_(pin2),
      c_(driver == faberlink::DRIVER_EN_IN1_IN2 ? pin3 : faberlink::NO_PIN),
      standby_(driver == faberlink::DRIVER_EN_IN1_IN2 ? pin4
               : driver == faberlink::DRIVER_TWO_PWM  ? pin3
                                                      : faberlink::NO_PIN),
      driver_(driver) {
  enqueue(this);
}

bool FaberMotor::wired() const {
  if (a_ == faberlink::NO_PIN || b_ == faberlink::NO_PIN) return false;
  return driver_ != faberlink::DRIVER_EN_IN1_IN2 || c_ != faberlink::NO_PIN;
}

void FaberMotor::begin() {
  output(a_);
  output(b_);
  output(c_);
  if (standby_ != faberlink::NO_PIN) {
    pinMode(standby_, OUTPUT);
    digitalWrite(standby_, HIGH);
  }
  drive(0);
}

uint8_t FaberMotor::apply(const uint8_t* command, uint8_t len, uint8_t* out, bool* clamped) {
  if (!wired()) return 0;  // refused: the board reports it, and the motor stays stopped
  return faberlink::Motor::apply(command, len, out, clamped);
}

// Every change writes the input that goes LOW before the one that goes HIGH, so the driver never
// sees both inputs HIGH (a brake) on the way from one direction to the other.
void FaberMotor::drive(float duty) {
  if (!wired()) return;
  float magnitude = duty < 0 ? -duty : duty;
  int speed = static_cast<int>(magnitude * 255.0f + 0.5f);
  if (driver_ == faberlink::DRIVER_TWO_PWM) {
    analogWrite(duty < 0 ? a_ : b_, 0);
    analogWrite(duty < 0 ? b_ : a_, speed);
    return;
  }
  if (driver_ == faberlink::DRIVER_EN_IN1_IN2) {
    uint8_t low = duty > 0 ? c_ : b_;  // forward: IN1 HIGH, IN2 LOW; reverse the other way
    digitalWrite(low, LOW);
    digitalWrite(low == b_ ? c_ : b_, duty == 0 ? LOW : HIGH);
  } else {
    digitalWrite(b_, duty < 0 ? HIGH : LOW);
  }
  analogWrite(a_, speed);
}

uint8_t FaberMotor::writePins(uint8_t* out) const {
  using namespace faberlink;
  const bool en = driver_ == DRIVER_EN_IN1_IN2, two = driver_ == DRIVER_TWO_PWM;
  const uint8_t pins[4] = {a_, b_, c_, standby_};
  const uint8_t roles[4] = {en ? ROLE_EN : two ? ROLE_IN1 : ROLE_PWM, en ? ROLE_IN1 : two ? ROLE_IN2 : ROLE_DIR,
                            ROLE_IN2, ROLE_STBY};
  uint8_t n = 0;
  for (uint8_t i = 0; i < 4; i++) n += putPin(out + n, roles[i], pins[i]);
  return n;
}

FaberDistance::FaberDistance(const char* name, uint8_t analogPin, float minM, float maxM,
                             faberlink::DistanceKind kind, ToMeters toMeters)
    : faberlink::Distance(name, kind, minM, maxM, 0.1f), toMeters_(toMeters), maxM_(maxM), pin_(analogPin) {
  enqueue(this);
}

uint8_t FaberDistance::writePins(uint8_t* out) const { return putPin(out, faberlink::ROLE_SIGNAL, pin_); }

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

uint8_t FaberUltrasonic::writePins(uint8_t* out) const {
  uint8_t n = putPin(out, faberlink::ROLE_TRIG, triggerPin_);
  return n + putPin(out + n, faberlink::ROLE_ECHO, echoPin_);
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

FaberButton::FaberButton(const char* name, uint8_t pin, faberlink::ActiveLevel active, uint16_t debounceMs)
    : faberlink::DigitalIn(name, debounceMs), pin_(pin), active_(active) {
  enqueue(this);
}

void FaberButton::begin() { input(pin_, active_); }

uint8_t FaberButton::writePins(uint8_t* out) const { return putPin(out, faberlink::ROLE_SIGNAL, pin_); }

bool FaberButton::readLevel() { return isActive(pin_, active_); }

FaberObstacle::FaberObstacle(const char* name, uint8_t pin, faberlink::ObstacleKind kind,
                             faberlink::ActiveLevel active, float triggerDistanceM)
    : faberlink::Obstacle(name, kind, triggerDistanceM), pin_(pin), active_(active) {
  enqueue(this);
}

void FaberObstacle::begin() { input(pin_, active_); }

uint8_t FaberObstacle::writePins(uint8_t* out) const { return putPin(out, faberlink::ROLE_SIGNAL, pin_); }

bool FaberObstacle::readLevel() { return isActive(pin_, active_); }

FaberOutput::FaberOutput(const char* name, uint8_t pin, faberlink::OutputMode mode, faberlink::ActiveLevel active)
    : faberlink::PinOutput(name, mode == faberlink::OUTPUT_PWM), pin_(pin), active_(active) {
  enqueue(this);
}

// Off before the pin becomes an output, so an active-low relay never clicks on at start.
void FaberOutput::begin() {
  digitalWrite(pin_, active_ == faberlink::ACTIVE_LOW ? HIGH : LOW);
  pinMode(pin_, OUTPUT);
}

uint8_t FaberOutput::writePins(uint8_t* out) const { return putPin(out, faberlink::ROLE_OUT, pin_); }

void FaberOutput::drive(float value) {
  const bool low = active_ == faberlink::ACTIVE_LOW;
  if (isPwm()) {
    int duty = static_cast<int>(value * 255.0f + 0.5f);
    analogWrite(pin_, low ? 255 - duty : duty);
  } else {
    digitalWrite(pin_, (value > 0) != low ? HIGH : LOW);
  }
}

FaberBattery::FaberBattery(const char* name, uint8_t analogPin, float dividerRatio, uint8_t cellsInSeries,
                           faberlink::BatteryChemistry chemistry, float referenceVolts, float capacityAh)
    : faberlink::Battery(name, chemistry, cellsInSeries, capacityAh),
      voltsPerCount_(referenceVolts * dividerRatio / 1023.0f),
      pin_(analogPin) {
  enqueue(this);
}

uint8_t FaberBattery::writePins(uint8_t* out) const { return putPin(out, faberlink::ROLE_SENSE, pin_); }

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
#if defined(ARDUINO_ARCH_AVR) && defined(PIN_A0)
  // On every AVR board the analog pins are numbered on from A0 (14 on an Uno, 54 on a Mega), so
  // Faber can show pin 14 as A0. Elsewhere that isn't always so (an ESP32's A4 is below its A0),
  // and Faber shows plain pin numbers.
  board_.setAnalogBase(PIN_A0);
#endif
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
