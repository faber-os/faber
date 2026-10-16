/**
 * The FaberLink library's public API (firmware/faberlink/src/FaberLink.h), for completion, hover
 * docs and signature help. faberlinkApi.test.ts reads the header and fails when a public class or
 * method appears or disappears there without changing here, so the two can't drift apart.
 */
import { fn, parseSignature, type ApiClass, type ApiConstant, type ApiFunction } from './catalogue';

const f = (signatures: string | string[], doc: string): ApiFunction =>
  fn(signatures, doc, 'faberlink');

/** Members every FaberLink part has (from `faberlink::Part` in FaberLinkCore.h). */
const PART_MEMBERS: ApiFunction[] = [
  f('const char* name() const', 'The part’s name, as Faber shows it after the board’s name.'),
];

const inherited = (list: ApiFunction[]) => list.map((m) => ({ ...m, inherited: true }));

export interface FaberLinkClass extends ApiClass {
  members: (ApiFunction & { inherited?: boolean })[];
}

export const FABERLINK_CLASSES: FaberLinkClass[] = [
  {
    name: 'FaberLink',
    doc: 'The connection to Faber over a serial port. Make one, call begin() in setup() and poll() in every loop(); the parts you declare add themselves.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature('FaberLink(HardwareSerial& port)').overload,
      parseSignature('FaberLink(Stream& port)').overload,
    ],
    members: [
      f(
        'void begin(unsigned long baud, const char* board, const char* firmwareVersion, uint16_t heartbeatTimeoutMs = faberlink::DEFAULT_HEARTBEAT_TIMEOUT_MS)',
        'Starts FaberLink: opens a hardware serial port at baud (115200 is usual) and tells Faber the board’s name and the sketch’s version. Without a heartbeat from Faber for heartbeatTimeoutMs (100 to 1000 ms), every motor stops.',
      ),
      f(
        'void poll()',
        'Does FaberLink’s work: reads Faber’s commands, sends readings, stops motors when Faber goes quiet. Call it in every loop() and keep loop() fast.',
      ),
      f(
        'bool add(faberlink::Part& part)',
        'Adds a part of your own (a faberlink::Part subclass). Call it before begin(); the Faber parts add themselves.',
      ),
      f(
        'void engageEstop()',
        'For a stop button on the board: stops every motor and stays stopped until resetEstop() here and a release in Faber.',
      ),
      f(
        'void resetEstop()',
        'Releases the board’s own stop; motors run again only after Faber releases too.',
      ),
      f('void log(const char* text)', 'Sends a line to Faber’s log.'),
      f('bool connected() const', 'True while Faber is connected.'),
    ],
  },
  {
    name: 'FaberMotor',
    doc: 'A DC motor on a motor driver (H-bridge). With two pins: a PWM pin sets the speed and a pin the direction (Cytron MD10C). For other drivers pass the driver first: faberlink::DRIVER_EN_IN1_IN2 with EN, IN1, IN2 and an optional STBY pin (L298N, L293D, TB6612), or faberlink::DRIVER_TWO_PWM with two PWM pins and an optional enable pin (L9110, DRV8833, BTS7960). Stopped, every speed and direction pin is LOW. Give maxVelocityRadS or call setMaxVelocity() (its speed at full power, in rad/s) to drive it by speed as well as by power.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberMotor(const char* name, uint8_t pwmPin, uint8_t directionPin, float maxVelocityRadS = 0)',
      ).overload,
      parseSignature(
        'FaberMotor(const char* name, faberlink::MotorDriver driver, uint8_t pin1, uint8_t pin2, uint8_t pin3 = faberlink::NO_PIN, uint8_t pin4 = faberlink::NO_PIN)',
      ).overload,
    ],
    members: [
      f('void begin()', 'Sets up its pins; FaberLink calls it from begin().'),
      ...inherited([
        ...PART_MEMBERS,
        f(
          'void setMaxVelocity(float radS)',
          'Its speed at full power, in rad/s: Faber can then drive it by speed as well as by power. Call it in setup().',
        ),
        f(
          'bool active() const',
          'True while it runs on a command from Faber, false while it is stopped (its safe state).',
        ),
        f(
          'float duty() const',
          'The power it runs at now, from -1 (full reverse) to 1 (full forward).',
        ),
      ]),
    ],
  },
  {
    name: 'FaberDistance',
    doc: 'An analog distance sensor on an analog pin. By default 0 V is 0 m and the reference voltage is maxM; for a sensor with a curve (a Sharp IR), pass a function from the analogRead() value to metres that returns a negative number for no target.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberDistance(const char* name, uint8_t analogPin, float minM = 0.1f, float maxM = 0.8f, faberlink::DistanceKind kind = faberlink::DISTANCE_INFRARED, ToMeters toMeters = 0)',
      ).overload,
    ],
    members: inherited(PART_MEMBERS),
  },
  {
    name: 'FaberUltrasonic',
    doc: 'An ultrasonic range finder with a trigger pin and an echo pin, such as the HC-SR04 (2 cm to 4 m). A measurement waits at most about 24 ms for its echo.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberUltrasonic(const char* name, uint8_t triggerPin, uint8_t echoPin, float minM = 0.02f, float maxM = 4.0f)',
      ).overload,
    ],
    members: [
      f('void begin()', 'Sets up its pins; FaberLink calls it from begin().'),
      ...inherited(PART_MEMBERS),
    ],
  },
  {
    name: 'FaberButton',
    doc: 'A button, switch or end stop on a digital pin, debounced. Wired between the pin and GND it needs nothing else (the pin’s pull-up holds it HIGH, pressed is LOW); pass faberlink::ACTIVE_HIGH for a part that drives the pin HIGH, such as a TTP223 touch pad. Faber sees it as an on/off input (digital-in).',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberButton(const char* name, uint8_t pin, faberlink::ActiveLevel active = faberlink::ACTIVE_LOW, uint16_t debounceMs = 20)',
      ).overload,
    ],
    members: [
      f('void begin()', 'Sets up its pin; FaberLink calls it from begin().'),
      f('bool pressed() const', 'True while it is pressed (debounced), Faber or not.'),
      ...inherited(PART_MEMBERS),
    ],
  },
  {
    name: 'FaberObstacle',
    doc: 'A bumper, whisker, limit switch or digital infrared obstacle module (FC-51, E18-D80NK) on a digital pin, debounced: it says whether something is in the way. Faber sees it as an obstacle sensor, so apps looking for a button never pick it.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberObstacle(const char* name, uint8_t pin, faberlink::ObstacleKind kind = faberlink::OBSTACLE_BUMPER, faberlink::ActiveLevel active = faberlink::ACTIVE_LOW, float triggerDistanceM = 0)',
      ).overload,
    ],
    members: [
      f('void begin()', 'Sets up its pin; FaberLink calls it from begin().'),
      f('bool detected() const', 'True while something is in the way (debounced), Faber or not.'),
      ...inherited(PART_MEMBERS),
    ],
  },
  {
    name: 'FaberOutput',
    doc: 'An output on one pin that Faber switches: on or off (an LED, a relay module, an active buzzer), or with faberlink::OUTPUT_PWM a duty from 0 to 1 on a PWM pin (a dimmed LED, a MOSFET). faberlink::ACTIVE_LOW makes LOW mean on (most relay modules). Leased like a motor: it goes off when nobody has commanded it for 1 s, on the e-stop and when Faber goes quiet.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberOutput(const char* name, uint8_t pin, faberlink::OutputMode mode = faberlink::OUTPUT_ON_OFF, faberlink::ActiveLevel active = faberlink::ACTIVE_HIGH)',
      ).overload,
    ],
    members: [
      f('void begin()', 'Sets up its pin, off; FaberLink calls it from begin().'),
      ...inherited([
        ...PART_MEMBERS,
        f(
          'float value() const',
          'The output now: 0 or 1 when on/off, the duty from 0 to 1 for PWM.',
        ),
        f(
          'bool active() const',
          'True while it runs on a command from Faber, false while it is off (its safe state).',
        ),
      ]),
    ],
  },
  {
    name: 'FaberServo',
    doc: 'A hobby servo (SG90, MG996R) that Faber turns to an angle, on any digital pin. Needs #include <FaberServo.h> after FaberLink.h (it uses the Servo library). Angles are degrees: minDeg and maxDeg are what minPulseUs and maxPulseUs (544 and 2400 µs by default) turn it to. It starts limp, and holds its angle when nobody drives it (faberlink::SERVO_RELEASE: goes limp). On an Uno, a servo stops analogWrite() on pins 9 and 10. Give it its own 5–6 V supply.',
    library: 'faberlink',
    include: 'FaberServo.h',
    constructors: [
      parseSignature(
        'FaberServo(const char* name, uint8_t pin, float minDeg = 0, float maxDeg = 180, faberlink::ServoSafeState safeState = faberlink::SERVO_HOLD, uint16_t minPulseUs = 544, uint16_t maxPulseUs = 2400)',
      ).overload,
    ],
    members: inherited([
      ...PART_MEMBERS,
      f('bool holding() const', 'True while it holds an angle (it gets pulses), false while limp.'),
      f('float angle() const', 'The angle it holds, in radians, as Faber last told it.'),
    ]),
  },
  {
    name: 'FaberBattery',
    doc: 'A battery measured through a voltage divider (battery + to R1 to the pin to R2 to GND). dividerRatio is (R1 + R2) / R2; keep the pin below referenceVolts when the battery is full.',
    library: 'faberlink',
    include: 'FaberLink.h',
    constructors: [
      parseSignature(
        'FaberBattery(const char* name, uint8_t analogPin, float dividerRatio, uint8_t cellsInSeries, faberlink::BatteryChemistry chemistry = faberlink::BATTERY_LI_ION, float referenceVolts = 5.0f, float capacityAh = 0)',
      ).overload,
    ],
    members: inherited([
      ...PART_MEMBERS,
      f(
        'faberlink::BatteryLevel level() const',
        'The level reported last: faberlink::BATTERY_OK, BATTERY_LOW or BATTERY_CRITICAL.',
      ),
    ]),
  },
];

const k = (name: string, doc: string): ApiConstant => ({
  name,
  doc,
  library: 'faberlink',
  namespace: 'faberlink',
});

/** The `faberlink::` constants a sketch passes to the classes above. */
export const FABERLINK_CONSTANTS: ApiConstant[] = [
  k('DISTANCE_ULTRASONIC', 'FaberDistance: an ultrasonic sensor.'),
  k('DISTANCE_LASER_TOF', 'FaberDistance: a laser time-of-flight sensor.'),
  k('DISTANCE_INFRARED', 'FaberDistance: an infrared sensor (the default).'),
  k('DISTANCE_RADAR', 'FaberDistance: a radar sensor.'),
  k('BATTERY_LI_ION', 'FaberBattery: lithium-ion cells, such as 18650s (the default).'),
  k('BATTERY_LIPO', 'FaberBattery: lithium-polymer cells.'),
  k('BATTERY_LIFEPO4', 'FaberBattery: lithium iron phosphate cells.'),
  k('BATTERY_NIMH', 'FaberBattery: nickel-metal hydride cells.'),
  k('BATTERY_LEAD_ACID', 'FaberBattery: a lead-acid battery.'),
  k(
    'DRIVER_PWM_DIR',
    'FaberMotor: a driver with a PWM (speed) pin and a direction pin, such as a Cytron MD10C. Pins: PWM, DIR.',
  ),
  k(
    'DRIVER_EN_IN1_IN2',
    'FaberMotor: a driver with EN (speed, PWM) and IN1/IN2 (direction), such as an L298N, L293D or TB6612. Pins: EN, IN1, IN2, then STBY if it has one.',
  ),
  k(
    'DRIVER_TWO_PWM',
    'FaberMotor: a driver with two PWM inputs, one per direction, such as an L9110, DRV8833 or BTS7960. Pins: IN1, IN2, then an enable pin if it has one.',
  ),
  k('NO_PIN', 'A pin that isn’t connected, for an optional pin you leave out.'),
  k(
    'ACTIVE_LOW',
    'FaberButton, FaberObstacle: LOW means pressed or detected, and the pin’s pull-up is on (a switch to GND); their default. FaberOutput: LOW means on (most relay modules).',
  ),
  k(
    'ACTIVE_HIGH',
    'FaberButton, FaberObstacle: HIGH means pressed or detected, no pull-up (a TTP223 touch pad). FaberOutput: HIGH means on; its default.',
  ),
  k('SERVO_HOLD', 'FaberServo: holds its angle when nobody drives it (the default).'),
  k('SERVO_RELEASE', 'FaberServo: goes limp (no pulses) when nobody drives it.'),
  k('OUTPUT_ON_OFF', 'FaberOutput: on or off, Faber sees a digital output (the default).'),
  k(
    'OUTPUT_PWM',
    'FaberOutput: a duty from 0 to 1 with analogWrite(), on a PWM pin; Faber sees a PWM output.',
  ),
  k('OBSTACLE_BUMPER', 'FaberObstacle: a bumper or whisker that something presses (the default).'),
  k('OBSTACLE_INFRARED', 'FaberObstacle: a digital infrared obstacle module (FC-51, E18-D80NK).'),
  k('OBSTACLE_LIMIT', 'FaberObstacle: a limit switch at the edge of where the machine may go.'),
  k('BATTERY_OK', 'FaberBattery level: charged enough.'),
  k('BATTERY_LOW', 'FaberBattery level: low, charge it soon.'),
  k('BATTERY_CRITICAL', 'FaberBattery level: very low; stop and charge it.'),
  k(
    'DEFAULT_HEARTBEAT_TIMEOUT_MS',
    'FaberLink.begin(): how long the board waits for Faber’s heartbeat before it stops the motors (250 ms).',
  ),
];
