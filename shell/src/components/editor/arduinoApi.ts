/**
 * The Arduino language as the editor knows it: the core's functions and constants, the ready-made
 * objects (`Serial`, `Wire`, `SPI`, `EEPROM`), the classes behind them, `String` and `Servo`, the
 * C++ keywords and the snippets. Data for completion, hover docs and signature help
 * (docs/arduino-editor.md, "Adding an API entry"). The descriptions are our own words, in
 * English like the code they describe.
 */
import {
  fn,
  parseSignature,
  type ApiClass,
  type ApiConstant,
  type ApiFunction,
  type ApiObject,
  type ApiType,
  type Overload,
  type Snippet,
} from './catalogue';

const ctor = (...signatures: string[]): Overload[] =>
  signatures.map((s) => parseSignature(s).overload);

const c = (name: string, doc: string): ApiConstant => ({ name, doc, library: 'arduino' });

export const ARDUINO_FUNCTIONS: ApiFunction[] = [
  // Digital pins.
  fn(
    'void pinMode(uint8_t pin, uint8_t mode)',
    'Makes a pin an INPUT, an OUTPUT or an INPUT_PULLUP (an input held high by a resistor inside the chip). Call it in setup().',
  ),
  fn(
    'void digitalWrite(uint8_t pin, uint8_t value)',
    'Sets an output pin HIGH (5 V on an Uno or a Mega) or LOW (0 V). On an input pin, HIGH turns the pull-up resistor on.',
  ),
  fn(
    'int digitalRead(uint8_t pin)',
    'Reads a digital pin: HIGH or LOW. A pin connected to nothing reads at random; use INPUT_PULLUP or a resistor.',
  ),
  // Analog pins.
  fn(
    'int analogRead(uint8_t pin)',
    'Measures the voltage on an analog pin (A0, A1 …) as a number from 0 (0 V) to 1023 (the reference voltage). Takes about 0.1 ms.',
  ),
  fn(
    'void analogWrite(uint8_t pin, int value)',
    'Sends a PWM signal on a pin marked ~: 0 is always off, 255 always on, 127 half the time. Dims an LED or sets a motor speed.',
  ),
  fn(
    'void analogReference(uint8_t mode)',
    'Picks the voltage analogRead() compares with: DEFAULT (the supply), INTERNAL (a built-in reference) or EXTERNAL (the AREF pin).',
  ),
  // Advanced I/O.
  fn(
    [
      'void tone(uint8_t pin, unsigned int frequency)',
      'void tone(uint8_t pin, unsigned int frequency, unsigned long duration)',
    ],
    'Plays a square wave of the given frequency in Hz on a pin, for a buzzer or a speaker, until noTone() or for duration ms. Uses a timer, so PWM on pins 3 and 11 stops on an Uno.',
  ),
  fn('void noTone(uint8_t pin)', 'Stops the sound tone() started on a pin.'),
  fn(
    [
      'unsigned long pulseIn(uint8_t pin, uint8_t state)',
      'unsigned long pulseIn(uint8_t pin, uint8_t state, unsigned long timeout)',
    ],
    'Waits for a pulse (HIGH or LOW) on a pin and returns how long it lasted in microseconds, or 0 when none came within timeout µs (1 s by default). Used for ultrasonic echoes.',
  ),
  fn(
    [
      'unsigned long pulseInLong(uint8_t pin, uint8_t state)',
      'unsigned long pulseInLong(uint8_t pin, uint8_t state, unsigned long timeout)',
    ],
    'Like pulseIn(), but measured with micros(), so it suits long pulses and needs interrupts on.',
  ),
  fn(
    'uint8_t shiftIn(uint8_t dataPin, uint8_t clockPin, uint8_t bitOrder)',
    'Reads a byte one bit at a time from a shift register, pulsing the clock pin for each bit. bitOrder is MSBFIRST or LSBFIRST.',
  ),
  fn(
    'void shiftOut(uint8_t dataPin, uint8_t clockPin, uint8_t bitOrder, uint8_t value)',
    'Writes a byte one bit at a time to a shift register such as the 74HC595. bitOrder is MSBFIRST or LSBFIRST.',
  ),
  // Time.
  fn(
    'void delay(unsigned long ms)',
    'Waits the given number of milliseconds. Nothing else in loop() runs meanwhile; with FaberLink, keep loop() free of delay() and use millis() instead.',
  ),
  fn(
    'void delayMicroseconds(unsigned int us)',
    'Waits the given number of microseconds (accurate from about 3 µs up to 16 383 µs).',
  ),
  fn(
    'unsigned long millis()',
    'Milliseconds since the board started. Wraps around to 0 after about 50 days; subtracting two readings (now - start) still works across that.',
  ),
  fn(
    'unsigned long micros()',
    'Microseconds since the board started, in steps of 4 µs on a 16 MHz board. Wraps around after about 70 minutes.',
  ),
  // Maths.
  fn('abs(x)', 'The value of x without its sign: abs(-3) is 3.'),
  fn(
    'constrain(x, low, high)',
    'Keeps x between low and high: returns low when x is smaller, high when it is larger.',
  ),
  fn(
    'long map(long value, long fromLow, long fromHigh, long toLow, long toHigh)',
    'Rescales a number from one range to another, e.g. map(analogRead(A0), 0, 1023, 0, 255). Works in whole numbers and does not keep the result inside the range.',
  ),
  fn('max(a, b)', 'The larger of two numbers.'),
  fn('min(a, b)', 'The smaller of two numbers.'),
  fn('double pow(double base, double exponent)', 'base raised to the power exponent.'),
  fn('sq(x)', 'x times x.'),
  fn('double sqrt(double x)', 'The square root of x.'),
  fn('double sin(double radians)', 'The sine of an angle in radians.'),
  fn('double cos(double radians)', 'The cosine of an angle in radians.'),
  fn('double tan(double radians)', 'The tangent of an angle in radians.'),
  fn(
    'double atan2(double y, double x)',
    'The angle of the point (x, y) from the x axis, in radians, from -PI to PI.',
  ),
  fn('radians(degrees)', 'Converts degrees to radians.'),
  fn('degrees(radians)', 'Converts radians to degrees.'),
  // Characters.
  fn('bool isAlpha(int c)', 'True for a letter.'),
  fn('bool isAlphaNumeric(int c)', 'True for a letter or a digit.'),
  fn('bool isAscii(int c)', 'True for a 7-bit ASCII character.'),
  fn('bool isControl(int c)', 'True for a control character such as a newline.'),
  fn('bool isDigit(int c)', 'True for a digit, 0 to 9.'),
  fn('bool isGraph(int c)', 'True for a printable character that is not a space.'),
  fn('bool isHexadecimalDigit(int c)', 'True for 0 to 9, a to f and A to F.'),
  fn('bool isLowerCase(int c)', 'True for a lower-case letter.'),
  fn('bool isPrintable(int c)', 'True for any printable character, a space included.'),
  fn(
    'bool isPunct(int c)',
    'True for punctuation: a printable character that is not a letter, a digit or a space.',
  ),
  fn(
    'bool isSpace(int c)',
    'True for white space: a space, a tab, a newline, a carriage return, a form feed or a vertical tab.',
  ),
  fn('bool isUpperCase(int c)', 'True for an upper-case letter.'),
  fn('bool isWhitespace(int c)', 'True for a space or a tab.'),
  // Random numbers.
  fn(
    ['long random(long max)', 'long random(long min, long max)'],
    'A pseudo-random number from min (0 if left out) up to, but not including, max.',
  ),
  fn(
    'void randomSeed(unsigned long seed)',
    'Starts random() at another point of its sequence. randomSeed(analogRead(A0)) on an unconnected pin gives a different sequence each start.',
  ),
  // Bits and bytes.
  fn('bit(n)', 'The value with only bit n set: bit(3) is 8.'),
  fn('bitClear(x, n)', 'Sets bit n of the variable x to 0.'),
  fn('bitRead(x, n)', 'Bit n of x: 0 or 1. Bit 0 is the rightmost.'),
  fn('bitSet(x, n)', 'Sets bit n of the variable x to 1.'),
  fn('bitWrite(x, n, b)', 'Sets bit n of the variable x to b (0 or 1).'),
  fn('highByte(x)', 'The high (left) byte of a 16-bit value.'),
  fn('lowByte(x)', 'The low (right) byte of a value.'),
  // Interrupts.
  fn(
    'void attachInterrupt(uint8_t interrupt, void (*isr)(), int mode)',
    'Runs the function isr as soon as a pin changes, whatever loop() is doing. Use digitalPinToInterrupt(pin) for the first argument; mode is LOW, CHANGE, RISING or FALLING. Keep isr short, and make variables it changes volatile.',
  ),
  fn(
    'void detachInterrupt(uint8_t interrupt)',
    'Stops calling the function attachInterrupt() set for this interrupt.',
  ),
  fn(
    'digitalPinToInterrupt(pin)',
    'The interrupt number of a pin, for attachInterrupt(). On an Uno only pins 2 and 3 have one; on a Mega 2, 3, 18, 19, 20 and 21.',
  ),
  fn(
    'void interrupts()',
    'Turns interrupts back on after noInterrupts(). millis() and Serial need them.',
  ),
  fn(
    'void noInterrupts()',
    'Turns interrupts off, so a few lines of code run without being interrupted. Turn them on again quickly with interrupts().',
  ),
];

export const ARDUINO_CONSTANTS: ApiConstant[] = [
  c('HIGH', 'A pin at the supply voltage (5 V on an Uno or a Mega), or a 1.'),
  c('LOW', 'A pin at 0 V, or a 0.'),
  c('INPUT', 'pinMode(): the pin reads, with nothing pulling it up or down.'),
  c('OUTPUT', 'pinMode(): the pin drives HIGH or LOW (at most 20 mA on an Uno or a Mega).'),
  c(
    'INPUT_PULLUP',
    'pinMode(): the pin reads, held HIGH by a resistor inside the chip, so a button to GND reads LOW when pressed.',
  ),
  c('LED_BUILTIN', 'The pin of the LED on the board: 13 on an Uno, a Nano or a Mega.'),
  c('CHANGE', 'attachInterrupt(): whenever the pin changes.'),
  c('RISING', 'attachInterrupt(): when the pin goes from LOW to HIGH.'),
  c('FALLING', 'attachInterrupt(): when the pin goes from HIGH to LOW.'),
  c('LSBFIRST', 'shiftIn(), shiftOut() and SPI: the lowest bit first.'),
  c('MSBFIRST', 'shiftIn(), shiftOut() and SPI: the highest bit first.'),
  c('DEC', 'print(): write the number in decimal (the default).'),
  c('HEX', 'print(): write the number in hexadecimal.'),
  c('OCT', 'print(): write the number in octal.'),
  c('BIN', 'print(): write the number in binary.'),
  c('DEFAULT', 'analogReference(): the supply voltage.'),
  c('INTERNAL', 'analogReference(): the chip’s own reference, 1.1 V on an Uno.'),
  c('INTERNAL1V1', 'analogReference(): the built-in 1.1 V reference (Mega only).'),
  c('INTERNAL2V56', 'analogReference(): the built-in 2.56 V reference (Mega only).'),
  c('EXTERNAL', 'analogReference(): the voltage on the AREF pin (0 to 5 V).'),
  c('PI', 'π, 3.14159…'),
  c('HALF_PI', 'Half of π, 1.5708…'),
  c('TWO_PI', 'Two times π, 6.28318…'),
  c('DEG_TO_RAD', 'Multiply degrees by this to get radians.'),
  c('RAD_TO_DEG', 'Multiply radians by this to get degrees.'),
  c('SERIAL_8N1', 'Serial.begin(): 8 data bits, no parity, 1 stop bit (the default).'),
  c('SPI_MODE0', 'SPISettings: clock idle low, data read on the rising edge.'),
  c('SPI_MODE1', 'SPISettings: clock idle low, data read on the falling edge.'),
  c('SPI_MODE2', 'SPISettings: clock idle high, data read on the falling edge.'),
  c('SPI_MODE3', 'SPISettings: clock idle high, data read on the rising edge.'),
  ...Array.from({ length: 16 }, (_, i) =>
    c(
      `A${i}`,
      i < 6
        ? `Analog pin ${i}, for analogRead() (also a digital pin).`
        : `Analog pin ${i}, for analogRead() (on a Mega; ${i < 8 ? 'a Nano has A6 and A7 for analogRead() only' : 'not on an Uno or a Nano'}).`,
    ),
  ),
];

/** Every Stream (Serial, Wire, a software serial port) reads and writes with these. */
const STREAM_MEMBERS: ApiFunction[] = [
  fn('int available()', 'How many bytes have arrived and wait to be read.'),
  fn('int read()', 'The next byte that arrived, or -1 when there is none.'),
  fn('int peek()', 'The next byte that arrived, without taking it; -1 when there is none.'),
  fn(
    ['size_t print(value)', 'size_t print(value, int format)'],
    'Writes a number or text as readable characters. For a whole number, format is DEC, HEX, OCT or BIN; for a float, how many decimals (2 by default).',
  ),
  fn(
    ['size_t println()', 'size_t println(value)', 'size_t println(value, int format)'],
    'Like print(), followed by a new line ("\\r\\n").',
  ),
  fn(
    [
      'size_t write(uint8_t byte)',
      'size_t write(const char* text)',
      'size_t write(const uint8_t* buffer, size_t length)',
    ],
    'Writes bytes as they are, not as readable digits: write(65) sends the byte 65 (the letter A).',
  ),
  fn('void flush()', 'Waits until everything written has been sent.'),
  fn(
    'void setTimeout(unsigned long ms)',
    'How long the read and parse functions wait for more data: 1000 ms by default.',
  ),
  fn(
    'long parseInt()',
    'Reads the next whole number, skipping what comes before it. Returns 0 after the timeout.',
  ),
  fn('float parseFloat()', 'Reads the next number with decimals, skipping what comes before it.'),
  fn(
    'bool find(const char* target)',
    'Reads until the text target has passed (true) or the timeout ends (false).',
  ),
  fn(
    'bool findUntil(const char* target, const char* terminator)',
    'Like find(), but also stops at the text terminator.',
  ),
  fn(
    'size_t readBytes(char* buffer, size_t length)',
    'Reads up to length bytes into buffer, until the timeout; returns how many it read.',
  ),
  fn(
    'size_t readBytesUntil(char terminator, char* buffer, size_t length)',
    'Like readBytes(), but stops at the terminator character (which is not stored).',
  ),
  fn('String readString()', 'Reads characters into a String until the timeout.'),
  fn(
    'String readStringUntil(char terminator)',
    'Reads characters into a String until the terminator character or the timeout.',
  ),
];

export const ARDUINO_CLASSES: ApiClass[] = [
  {
    name: 'Stream',
    doc: 'Anything that reads and writes bytes one after another: a serial port, Wire, a software serial port.',
    library: 'arduino',
    constructors: [],
    members: STREAM_MEMBERS,
  },
  {
    name: 'HardwareSerial',
    doc: 'A serial port of the chip: Serial on the USB cable (pins 0 and 1), and Serial1 to Serial3 on a Mega.',
    library: 'arduino',
    extends: 'Stream',
    constructors: [],
    members: [
      fn(
        ['void begin(unsigned long baud)', 'void begin(unsigned long baud, uint8_t config)'],
        'Opens the port at the given speed in bits per second, e.g. 9600 or 115200. The other side must use the same speed.',
      ),
      fn('void end()', 'Closes the port, so its pins work as normal pins again.'),
      fn('int availableForWrite()', 'How many bytes can be written without waiting.'),
    ],
  },
  {
    name: 'String',
    doc: 'Text that can grow and shrink. Easy to use, but it allocates memory: on an Uno, prefer fixed char arrays in code that runs for a long time.',
    library: 'arduino',
    constructors: ctor(
      'String(const char* text)',
      'String(int value, unsigned char base)',
      'String(float value, unsigned char decimals)',
    ),
    members: [
      fn('unsigned int length() const', 'How many characters the text has.'),
      fn('char charAt(unsigned int index) const', 'The character at index (0 is the first).'),
      fn('void setCharAt(unsigned int index, char c)', 'Replaces the character at index.'),
      fn(
        [
          'String substring(unsigned int from) const',
          'String substring(unsigned int from, unsigned int to) const',
        ],
        'The part from index from up to, but not including, index to (the end if left out).',
      ),
      fn(
        ['int indexOf(char c) const', 'int indexOf(const String& text, unsigned int from) const'],
        'Where a character or a text first appears, from the start or from index from; -1 if it doesn’t.',
      ),
      fn(
        ['int lastIndexOf(char c) const', 'int lastIndexOf(const String& text) const'],
        'Where a character or a text last appears; -1 if it doesn’t.',
      ),
      fn('bool startsWith(const String& prefix) const', 'True when the text begins with prefix.'),
      fn('bool endsWith(const String& suffix) const', 'True when the text ends with suffix.'),
      fn(
        'bool equals(const String& other) const',
        'True when both texts are the same (== works too).',
      ),
      fn(
        'bool equalsIgnoreCase(const String& other) const',
        'Like equals(), but A and a count as the same.',
      ),
      fn(
        'int compareTo(const String& other) const',
        'Below 0 if this text sorts before other, 0 if equal, above 0 if after.',
      ),
      fn('bool concat(value)', 'Adds a number or text to the end (+= works too).'),
      fn(
        'void replace(const String& find, const String& replacement)',
        'Replaces every find in the text with replacement.',
      ),
      fn(
        ['void remove(unsigned int index)', 'void remove(unsigned int index, unsigned int count)'],
        'Removes count characters from index (all the rest if left out).',
      ),
      fn(
        'bool reserve(unsigned int size)',
        'Sets aside memory for a text of this length, so adding to it later doesn’t fragment memory.',
      ),
      fn('long toInt() const', 'The whole number the text starts with, or 0.'),
      fn('float toFloat() const', 'The number the text starts with, or 0.'),
      fn(
        'double toDouble() const',
        'The number the text starts with, or 0 (a float on AVR boards).',
      ),
      fn('void toLowerCase()', 'Changes the text to lower case.'),
      fn('void toUpperCase()', 'Changes the text to upper case.'),
      fn('void trim()', 'Removes spaces, tabs and newlines from both ends.'),
      fn(
        'const char* c_str() const',
        'The text as a C string, for functions that take a const char*.',
      ),
      fn(
        'void toCharArray(char* buffer, unsigned int length) const',
        'Copies the text into a char array of the given size.',
      ),
      fn(
        'void getBytes(unsigned char* buffer, unsigned int length) const',
        'Copies the text’s bytes into buffer.',
      ),
    ],
  },
  {
    name: 'TwoWire',
    doc: 'The I²C bus (SDA and SCL pins: A4 and A5 on an Uno, 20 and 21 on a Mega), for sensors and displays that share two wires.',
    library: 'arduino',
    include: 'Wire.h',
    extends: 'Stream',
    constructors: [],
    members: [
      fn(
        ['void begin()', 'void begin(uint8_t address)'],
        'Joins the bus: as the controller, or with an address as a peripheral. Call it once in setup().',
      ),
      fn('void end()', 'Leaves the bus.'),
      fn(
        'void setClock(uint32_t hz)',
        'The bus speed: 100000 (standard, the default) or 400000 (fast).',
      ),
      fn(
        'void beginTransmission(uint8_t address)',
        'Starts a message to the device at address. Queue bytes with write(), send with endTransmission().',
      ),
      fn(
        ['uint8_t endTransmission()', 'uint8_t endTransmission(bool stop)'],
        'Sends the queued bytes. Returns 0 on success; 2 means no device answered at that address.',
      ),
      fn(
        [
          'uint8_t requestFrom(uint8_t address, uint8_t quantity)',
          'uint8_t requestFrom(uint8_t address, uint8_t quantity, bool stop)',
        ],
        'Asks the device at address for quantity bytes; read them with available() and read().',
      ),
      fn(
        'void onReceive(void (*handler)(int count))',
        'As a peripheral: the function to call when the controller sends bytes.',
      ),
      fn(
        'void onRequest(void (*handler)())',
        'As a peripheral: the function to call when the controller asks for bytes.',
      ),
      fn(
        [
          'void setWireTimeout(uint32_t timeoutUs)',
          'void setWireTimeout(uint32_t timeoutUs, bool resetWithTimeout)',
        ],
        'Gives up a transfer after timeoutUs µs instead of hanging when the bus gets stuck.',
      ),
    ],
  },
  {
    name: 'SPIClass',
    doc: 'The SPI bus (pins 11, 12, 13 on an Uno; 51, 50, 52 on a Mega), a fast bus for SD cards, displays and radios.',
    library: 'arduino',
    include: 'SPI.h',
    constructors: [],
    members: [
      fn('void begin()', 'Starts the bus. Call it once in setup().'),
      fn('void end()', 'Stops the bus.'),
      fn(
        'void beginTransaction(SPISettings settings)',
        'Takes the bus with a device’s speed, bit order and mode, e.g. SPISettings(4000000, MSBFIRST, SPI_MODE0). Then pull its chip-select pin LOW.',
      ),
      fn('void endTransaction()', 'Gives the bus back after beginTransaction().'),
      fn(
        ['uint8_t transfer(uint8_t data)', 'void transfer(void* buffer, size_t count)'],
        'Sends a byte (or a buffer) and returns what came back at the same time.',
      ),
      fn('uint16_t transfer16(uint16_t data)', 'Sends and receives 16 bits at once.'),
      fn(
        'void usingInterrupt(uint8_t interrupt)',
        'Tells SPI that an interrupt uses the bus, so a transaction holds it off.',
      ),
    ],
  },
  {
    name: 'SPISettings',
    doc: 'A device’s SPI speed, bit order and mode, for SPI.beginTransaction().',
    library: 'arduino',
    include: 'SPI.h',
    constructors: ctor('SPISettings(uint32_t clockHz, uint8_t bitOrder, uint8_t dataMode)'),
    members: [],
  },
  {
    name: 'EEPROMClass',
    doc: 'The chip’s EEPROM: memory that keeps its bytes when the power is off (1 KB on an Uno, 4 KB on a Mega). Each byte survives about 100 000 writes.',
    library: 'arduino',
    include: 'EEPROM.h',
    constructors: [],
    members: [
      fn('uint8_t read(int address)', 'The byte at address.'),
      fn('void write(int address, uint8_t value)', 'Writes a byte at address (3.3 ms).'),
      fn(
        'void update(int address, uint8_t value)',
        'Writes a byte only when it differs from what is there, which saves the EEPROM.',
      ),
      fn('T& get(int address, T& value)', 'Reads any variable or struct stored from address.'),
      fn(
        'const T& put(int address, const T& value)',
        'Stores any variable or struct from address, writing only the bytes that changed.',
      ),
      fn('uint16_t length()', 'How many bytes the EEPROM has.'),
    ],
  },
  {
    name: 'Servo',
    doc: 'A hobby servo on a pin: turns to an angle and holds it. Up to 12 on an Uno; using it turns PWM off on pins 9 and 10.',
    library: 'arduino',
    include: 'Servo.h',
    constructors: ctor('Servo()'),
    members: [
      fn(
        ['uint8_t attach(int pin)', 'uint8_t attach(int pin, int minUs, int maxUs)'],
        'Connects the servo to a pin. minUs and maxUs are its pulse widths at 0 and 180 degrees (544 and 2400 µs by default).',
      ),
      fn('void detach()', 'Stops the pulses, so the servo goes limp.'),
      fn('void write(int angle)', 'Turns to an angle from 0 to 180 degrees.'),
      fn(
        'void writeMicroseconds(int us)',
        'Sets the pulse width directly, about 1000 to 2000 µs, for finer control.',
      ),
      fn('int read()', 'The angle written last.'),
      fn('int readMicroseconds()', 'The pulse width written last, in µs.'),
      fn('bool attached()', 'True while the servo is attached to a pin.'),
    ],
  },
];

export const ARDUINO_OBJECTS: ApiObject[] = [
  {
    name: 'Serial',
    type: 'HardwareSerial',
    doc: 'The serial port on the USB cable (pins 0 and 1). On a FaberLink board it belongs to FaberLink: don’t print to it.',
    library: 'arduino',
  },
  ...[1, 2, 3].map((n): ApiObject => ({
    name: `Serial${n}`,
    type: 'HardwareSerial',
    doc: `The Mega’s serial port ${n} (pins ${[18, 16, 14][n - 1]} TX and ${[19, 17, 15][n - 1]} RX).`,
    library: 'arduino',
  })),
  {
    name: 'Wire',
    type: 'TwoWire',
    doc: 'The I²C bus, for sensors and displays on two wires.',
    library: 'arduino',
    include: 'Wire.h',
  },
  { name: 'SPI', type: 'SPIClass', doc: 'The SPI bus.', library: 'arduino', include: 'SPI.h' },
  {
    name: 'EEPROM',
    type: 'EEPROMClass',
    doc: 'Memory that keeps its bytes without power.',
    library: 'arduino',
    include: 'EEPROM.h',
  },
];

export const ARDUINO_TYPES: ApiType[] = [
  { name: 'byte', doc: 'A number from 0 to 255 (8 bits, unsigned).' },
  { name: 'boolean', doc: 'The Arduino name for bool: true or false.' },
  { name: 'word', doc: 'A number from 0 to 65 535 (16 bits, unsigned).' },
  { name: 'size_t', doc: 'A size or a count of bytes.' },
  { name: 'int8_t', doc: '8 bits, signed: -128 to 127.' },
  { name: 'uint8_t', doc: '8 bits, unsigned: 0 to 255.' },
  { name: 'int16_t', doc: '16 bits, signed: -32 768 to 32 767 (an int on an Uno or a Mega).' },
  { name: 'uint16_t', doc: '16 bits, unsigned: 0 to 65 535.' },
  { name: 'int32_t', doc: '32 bits, signed (a long on an Uno or a Mega).' },
  { name: 'uint32_t', doc: '32 bits, unsigned: 0 to 4 294 967 295.' },
  { name: 'int64_t', doc: '64 bits, signed. Slow on an 8-bit board.' },
  { name: 'uint64_t', doc: '64 bits, unsigned. Slow on an 8-bit board.' },
];

/** C++ keywords and built-in types, offered as plain words. */
export const CPP_KEYWORDS: string[] = [
  'auto',
  'bool',
  'break',
  'case',
  'char',
  'class',
  'const',
  'constexpr',
  'continue',
  'default',
  'delete',
  'do',
  'double',
  'else',
  'enum',
  'explicit',
  'extern',
  'false',
  'float',
  'for',
  'friend',
  'goto',
  'if',
  'inline',
  'int',
  'long',
  'namespace',
  'new',
  'nullptr',
  'NULL',
  'operator',
  'override',
  'private',
  'protected',
  'public',
  'return',
  'short',
  'signed',
  'sizeof',
  'static',
  'struct',
  'switch',
  'template',
  'this',
  'true',
  'typedef',
  'typename',
  'union',
  'unsigned',
  'using',
  'virtual',
  'void',
  'volatile',
  'while',
];

/** Preprocessor directives, offered after `#`. */
export const DIRECTIVES: string[] = [
  'include',
  'define',
  'undef',
  'if',
  'ifdef',
  'ifndef',
  'elif',
  'else',
  'endif',
  'pragma once',
  'error',
  'warning',
];

/** Headers offered after `#include <`. */
export const HEADERS: string[] = [
  'Arduino.h',
  'FaberLink.h',
  'Wire.h',
  'SPI.h',
  'EEPROM.h',
  'Servo.h',
  'SoftwareSerial.h',
  'avr/pgmspace.h',
  'math.h',
  'string.h',
  'stdlib.h',
];

export const SNIPPETS: Snippet[] = [
  {
    label: 'setup',
    key: 'arduino.assist.snippet.setup',
    template: 'void setup() {\n\t${}\n}\n\nvoid loop() {\n\t\n}',
  },
  {
    label: 'for',
    key: 'arduino.assist.snippet.for',
    template: 'for (int ${i} = 0; ${i} < ${count}; ${i}++) {\n\t${}\n}',
  },
  { label: 'if', key: 'arduino.assist.snippet.if', template: 'if (${condition}) {\n\t${}\n}' },
  {
    label: 'ifelse',
    key: 'arduino.assist.snippet.ifelse',
    template: 'if (${condition}) {\n\t${}\n} else {\n\t\n}',
  },
  {
    label: 'switch',
    key: 'arduino.assist.snippet.switch',
    template: 'switch (${value}) {\n\tcase ${1}:\n\t\t${}\n\t\tbreak;\n\tdefault:\n\t\tbreak;\n}',
  },
  {
    label: 'while',
    key: 'arduino.assist.snippet.while',
    template: 'while (${condition}) {\n\t${}\n}',
  },
  {
    label: 'every',
    key: 'arduino.assist.snippet.every',
    template:
      'static unsigned long ${last} = 0;\nif (millis() - ${last} >= ${1000}) {\n\t${last} = millis();\n\t${}\n}',
  },
];
