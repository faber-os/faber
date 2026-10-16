# Research: the parts people connect to Arduinos and Raspberry Pis

Status: **research note** (2026-10-16), input to the guided mode in [../wiring.md](../wiring.md).
It answers: which parts must guided setup support, in what order, and what Faber (capabilities,
FaberLink classes, pin maps, allocator rules) is missing to support them.

## 0. How the ranking was made, and how far to trust it

There is **no public download ranking** for the Arduino Library Manager or for parts. Arduino
publishes none, and the third-party tracker arduinolibraries.info only shows GitHub stars and
forks. Reddit and forum frequency could not be measured in a reliable way. The tiers below are
therefore a judgement built from **overlap**: how many of these sources include the part.

| Source | What it tells us |
|---|---|
| Starter kits: Elegoo Super Starter / Mega "Most Complete", official Arduino Starter Kit, Keyestudio 37-in-1 V2.0, SunFounder Universal Maker Sensor Kit, Freenove Ultimate (Pi) | What beginners own, so what they will try first |
| Robot kits: Elegoo Smart Robot Car V4, Elegoo Tumbller | What a beginner robot is made of |
| Wokwi simulator part catalogue | What a simulator maker judged worth simulating, a strong "most used" signal |
| ESPHome component list, Johnny-Five API classes | What comparable "no-code" tools chose to support, and how they group parts |
| arduinolibraries.info stars/forks | Which libraries are widely used (stars and forks, not downloads) |

**Tier 1**: in almost every starter kit **and** in Wokwi/Johnny-Five/ESPHome; the parts beginners
ask about most. **Tier 2**: common, in many kits or in all the tools. **Tier 3**: niche for
beginners, but important for robots like SENNEP.

Where a fact comes from general knowledge of datasheets and library sources, not from the cited
pages, it is marked *(general knowledge, verify)*. Elegoo's own tutorial page was rate-limited during
this research, so parts of the Elegoo kit contents also come from general knowledge.

---

## 1. Kit overlap at a glance

✔ = in the source. The robot column is the Elegoo Smart Robot Car V4 / Tumbller.

| Part | Elegoo kits | Arduino Starter Kit | Keyestudio 37 | SunFounder UMSK | Freenove (Pi) | Robot kits | Wokwi | Johnny-Five | ESPHome |
|---|---|---|---|---|---|---|---|---|---|
| LED / RGB LED | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
| Push button | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
| Potentiometer | ✔ | ✔ | ✔ | ✔ | ✔ | | ✔ | ✔ | ✔ (ADC) |
| Photoresistor (LDR) | ✔ | ✔ | ✔ | ✔ | ✔ | | ✔ | ✔ | ✔ |
| Buzzer (active/passive) | ✔ | ✔ (piezo) | ✔ | ✔ | ✔ | | ✔ | ✔ | ✔ |
| HC-SR04 ultrasonic | ✔ | | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
| SG90 servo | ✔ | ✔ | | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
| DC motor + driver | ✔ (L293D) | ✔ (L293D) | | ✔ (L9110) | ✔ | ✔ (TB6612) | | ✔ | ✔ (H-bridge, TB6612) |
| Stepper 28BYJ-48 + ULN2003 | ✔ | | | | ✔ | | ✔ (A4988) | ✔ | ✔ |
| DHT11 / DHT22 | ✔ | | ✔ | ✔ | ✔ | | ✔ | ✔ | ✔ |
| 16×2 LCD (often I²C) | ✔ | ✔ | | ✔ | ✔ | | ✔ | ✔ | ✔ |
| 7-segment / TM1637 / 74HC595 | ✔ | | | | ✔ | | ✔ | ✔ | ✔ |
| IR receiver + remote | ✔ | | ✔ | | | ✔ | ✔ | | (remote_receiver) |
| Analog joystick | ✔ | | ✔ | | ✔ | | ✔ | ✔ | |
| 4×4 keypad | ✔ | | | | ✔ | | ✔ | ✔ | (TTP229) |
| PIR HC-SR501 | ✔ | | ✔ | ✔ | ✔ | | ✔ | ✔ | ✔ |
| Relay module | ✔ | | ✔ | ✔ | ✔ | | ✔ | ✔ | ✔ |
| MPU6050 (GY-521) | ✔ (Mega kit) | | (MMA8452Q) | ✔ | ✔ (accel) | ✔ (Tumbller) | ✔ | ✔ | ✔ |
| RC522 RFID | ✔ (Mega kit) | | | | | | ✔ | | ✔ |
| MAX7219 matrix | ✔ (Mega kit) | | | | ✔ (matrix) | | ✔ | ✔ | ✔ |
| RTC DS1307/DS3231 | ✔ | | ✔ | | | | ✔ | | ✔ |
| Line-tracking IR sensor | | | ✔ | | | ✔ | | ✔ (ReflectanceArray) | |
| IR obstacle sensor | | | ✔ | ✔ | | ✔ | | | |
| Rotary encoder KY-040 | | | ✔ | | | | ✔ | | ✔ |
| DS18B20 1-Wire | | | | ✔ | | | ✔ | ✔ | ✔ |
| BMP180/BMP280 | | | | ✔ | | | ✔ | ✔ | ✔ |
| HX711 load cell | | | | | | | ✔ | | ✔ |
| MQ-2 gas | | | ✔ | | | | ✔ | | (via ADC) |
| SSD1306 OLED | | | | ✔ | | | ✔ | | ✔ |
| WS2812 / NeoPixel | | | | | | | ✔ | | ✔ |
| VL53L0X ToF | | | | ✔ | | | | | ✔ |

Library signals (arduinolibraries.info, stars/forks, not downloads): FastLED and IRremote are in the
top 10 by stars; Adafruit GFX, MFRC522 and the DHT sensor library are in the top 10 by forks, so
displays, RFID and the DHT are among the most-used part libraries.

---

## 2. Per-part reference

Conventions in the tables: **Pins** lists each role with its requirement. "any DIO" = any digital
pin (on a Nano not A6/A7). **5V/3V3** = supply and logic level; "5 V-tolerant" means its inputs
accept 5 V logic. **Cap.** = Faber capability; ★ marks a capability that does not exist yet (§4).
**Pi** = direct use on the Pi 5 header. Library licences are given only where easy to confirm.

### 2.1 Distance and proximity

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **HC-SR04** ultrasonic (HC-SR04P 3–5.5 V, JSN-SR04T / AJ-SR04M waterproof, US-100 in GPIO mode, SRF05) | 1 | Trig: any DIO out. Echo: any DIO in; a pin-change or external interrupt pin allows non-blocking reads. Trig+Echo may share one pin (NewPing "one pin" mode) | 5 V, ~15 mA; Echo outputs 5 V | none (`pulseIn`), or NewPing (GPL-3.0) for timer-driven pings | `distance` (ultrasonic) — `FaberUltrasonic` exists | Echo is 5 V: needs a divider (or HC-SR04P at 3.3 V). Timing jitter under Linux; better on the Arduino |
| **Sharp IR** GP2Y0A21 (10–80 cm), GP2Y0A02 (20–150 cm), GP2Y0A41 (4–30 cm) | 2 | Out: analog in | 5 V, ~30 mA in bursts | none (curve per model) | `distance` (infrared) — `FaberDistance` exists | Needs an ADC (ADS1115/MCP3008) |
| **VL53L0X** ToF (GY-VL53L0XV2, Pololu, Adafruit) / **VL53L1X** (up to ~4 m) | 3 (robots: 1) | SDA/SCL: the board's I²C pins. XSHUT: any DIO out, **needed per sensor when more than one** (all start at 0x29). GPIO1 (interrupt): optional | Most breakouts take 2.6–5 V with a regulator; XSHUT on some carriers is **not** level-shifted (Pololu: 3.3 V only) | Pololu VL53L0X / VL53L1X, Adafruit_VL53L0X, SparkFun VL53L1X | `distance` (laser-tof) | Works on I²C directly; address dance via GPIOs |
| **TF-Luna / TFmini-S / TFmini Plus** (Benewake) | 3 | UART (default) TX/RX: a hardware serial port; or I²C (mode switch, default address 0x10) *(general knowledge, verify)* | 5 V, ~70–150 mA; TTL logic is 3.3 V (5 V-tolerant per datasheets, verify) | TFMPlus / TFMini libraries, or a few lines of frame parsing | `distance` (laser-tof) | Fine on a Pi UART; on an Uno it needs SoftwareSerial (weak) |
| **IR obstacle module** FC-51 / KY-032 / E18-D80NK | 2 | Out: any DIO in (active low); some have an EN pin | 3.3–5 V (E18-D80NK: 5 V, NPN open collector) | none | ★ `obstacle` (or `digital-in`) | Yes at 3.3 V; E18-D80NK needs a pull-up to 3.3 V |
| **LD19 / LD06** 360° LiDAR | 3 | UART TX at 230400 baud (one way) + PWM in for motor speed (optional) | 5 V, ~180–300 mA | Faber's own Rust stack | `laser-scan` (already supported) | Yes, its natural home is the Pi |
| **PIR** HC-SR501 / AM312 | 1 | Out: any DIO in | HC-SR501: 5–20 V in, 3.3 V output; AM312: 2.7–12 V | none | `presence` | Yes, the 3.3 V output is Pi-safe |
| **mmWave radar** LD2410 | 3 | UART | 5 V, logic 3.3 V | ld2410 libraries | `presence` (distance-m) | Yes |

**Gotchas**
- Several HC-SR04s fired together hear each other's echoes: fire them in turn, ≥ 60 ms apart for one
  sensor (datasheet), or interleave. `FaberUltrasonic` blocks up to ~24 ms per reading, so 4 sonars
  can block `loop()` for ~100 ms. At 115200 baud the 64-byte serial RX buffer fills in about 5.5 ms of
  continuous traffic, so a long blocking read can overflow it if Faber sends a burst. Interrupt-driven
  echo timing (pin-change interrupts) avoids this and is worth doing before SENNEP's 4 sonars.
- Multiple VL53L0X: hold every XSHUT low, release one, give it a new address, repeat. **The address is
  lost on every power-up**, so the sketch must do this on every boot. That is a natural job for a
  FaberLink class (the template gets one XSHUT role per sensor).
- Sharp IR sensors draw current in pulses and add noise to the 5 V rail; a 10 µF+ capacitor at the
  sensor is commonly recommended *(general knowledge)*.

### 2.2 Motion and position

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **MPU6050** (GY-521 board), MPU6500 | 1 (kits) | SDA/SCL. AD0: address 0x68 (low/open) or 0x69 (high). INT: optional, an interrupt pin for the DMP's data-ready | GY-521 has a 3.3 V regulator; VCC 5 V is fine, I²C pulled up to 3.3 V on board | ElectronicCats MPU6050 (i2cdevlib, MIT), Adafruit MPU6050, MPU6050_light | `imu` (no orientation unless the DMP is used) | Yes, I²C |
| **MPU9250 / MPU9255** (9-axis) | 2 | as MPU6050; magnetometer AK8963 at 0x0C behind the chip | 3.3 V chip, boards with regulator | Bolder Flight MPU9250, SparkFun | `imu` + `magnetic-field` | Yes |
| **BNO055** (Adafruit, GY-BNO055) | 3 (robots: 1) | SDA/SCL, address 0x28 or 0x29 (ADR pin). RST, INT optional. Uses **I²C clock stretching** | 3.3 V chip; Adafruit board takes 3–5 V | Adafruit_BNO055 + Adafruit Unified Sensor | `imu` with `orientation` (on-chip fusion) | The Pi 4 and earlier have a known clock-stretching problem; Pi 5 uses a different I²C controller *(verify on the Pi 5)*; UART mode is an alternative |
| **ICM-20948** (SparkFun, Adafruit) | 3 | SDA/SCL 0x68/0x69 (or SPI) | **3.3 V** chip; SparkFun breakout is 3.3 V only, Adafruit's has level shifting | SparkFun ICM-20948 | `imu` + `magnetic-field` | Yes |
| **BNO085 / BNO086** | 3 | I²C 0x4A/0x4B, SPI or UART-RVC | 3.3 V | Adafruit BNO08x, SparkFun | `imu` with orientation | Yes |
| **Wheel encoders** (hall or optical quadrature on gear motors, e.g. JGB37-520/JGA25 with hall encoders; slotted-disc LM393 "speed sensor" modules) | 3 (robots: 1) | A: **external-interrupt pin** (best) or pin-change pin. B: same for full 4× resolution, or any DIO for 2× | 3.3–5 V, few mA; hall encoders often have open-collector outputs that need pull-ups | Encoder (Paul Stoffregen / PJRC), or a FaberLink ISR | `encoder` (and `motor` velocity) | Not for wheel encoders (missed edges under Linux); fine for a slow knob |
| **Rotary encoder** KY-040 / EC11 | 2 | CLK, DT: interrupt or pin-change pins (or polled); SW: any DIO in | 3.3–5 V | Encoder, RotaryEncoder (mathertel) | `encoder` + `button` | Yes (slow) |
| **GPS** NEO-6M / NEO-M8N (GY-GPS6MV2) | 2 | TX → board RX (a hardware serial port), RX ← board TX (optional) | 3.3–5 V module, logic 3.3 V; 9600 baud default | TinyGPS++ | `position` | Yes, best on a Pi UART |
| **Compass** HMC5883L (GY-271) / QMC5883L | 2 | SDA/SCL; HMC 0x1E, QMC 0x0D | 3.3–5 V boards | QMC5883LCompass, Adafruit HMC5883 | `magnetic-field` | Yes |

**Gotchas**
- MPU6050 and MPU9250 are end-of-life at InvenSense, and many cheap boards carry clones or
  substitutes (a "MPU9250" board that is really an MPU6500 with no magnetometer is a frequent forum
  complaint). Reading WHO_AM_I and reporting it in the part's info would catch this *(general
  knowledge)*.
- **0x68 clash**: MPU6050/MPU9250/ICM-20948 and the DS3231/DS1307 RTC all use 0x68. With an RTC on
  the bus, the IMU's AD0 must go high (0x69). The allocator should catch this.
- Encoders: an Uno has **only two** external-interrupt pins (D2, D3); four wheel encoders need a
  Mega or pin-change interrupts. On a Mega, the six external interrupts are 2, 3, 18, 19, 20, 21, but
  **20/21 are I²C** and **18/19 are Serial1**, so a Mega with I²C sensors and one free hardware serial
  port has about four real interrupt pins (2, 3, 18, 19 if Serial1 is not used). Pin-change
  interrupts on the Mega exist only on 10–13, 50–53 and A8–A15.
- The count rate is CPR × 4 × gear ratio × wheel rev/s. An AVR interrupt costs microseconds, so the
  total edge rate across all encoders has a practical ceiling in the tens of kHz *(general knowledge;
  measure before relying on it)*.

### 2.3 Environment

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **DHT11 / DHT22 (AM2302)** | 1 | Data: any DIO, 10 kΩ pull-up (modules have it) | 3.3–5 V | DHT sensor library (Adafruit, MIT) | `temperature` + `humidity` (two named instances) | Bit-banged timing; works on a Pi but is unreliable under load |
| **DS18B20** (TO-92, waterproof probe) | 2 | Data: any DIO, **4.7 kΩ pull-up** to VCC; many sensors on one wire | 3–5.5 V; parasite mode possible | OneWire + DallasTemperature | `temperature` | Yes, kernel `w1-gpio` overlay |
| **LM35 / TMP36 / NTC thermistor** | 1 | Out: analog in | 4–30 V (LM35), 2.7–5.5 V (TMP36) | none | `temperature` | Needs an ADC |
| **BMP280 / BME280 / BMP180** | 2 | SDA/SCL; BMx280 0x76 or 0x77 (SDO), BMP180 0x77 fixed | Bare modules are 3.3 V; "5 V" modules have a regulator | Adafruit BMP280/BME280 | `pressure` + `temperature` (+ `humidity` for BME280) | Yes |
| **Photoresistor (LDR)** / TEMT6000 | 1 | Out: analog in, LDR in a divider with ~10 kΩ | any | none | `illuminance` (uncalibrated) or `analog-in` | Needs an ADC |
| **BH1750** | 2 | SDA/SCL 0x23 or 0x5C | 3.3–5 V module | BH1750 (claws) | `illuminance` | Yes |
| **MQ-2 / MQ-135 / MQ-x gas** | 2 | AO: analog in; DO: any DIO (comparator) | 5 V, **~150 mA heater**, needs a burn-in | none / MQUnifiedsensor | `concentration` (uncalibrated) | Needs an ADC; the heater load matters on a 5 V pin |
| **Soil moisture** (capacitive v1.2; resistive YL-69) | 2 | AO: analog in | 3.3–5 V | none | `soil-moisture` | Needs an ADC |
| **Sound sensor** KY-037/038, MAX4466 | 2 | AO: analog; DO: digital | 3.3–5 V | none | `sound-level` (relative) or `digital-in` | Needs an ADC |
| **Flame / Hall / reed / tilt / vibration (SW-420) / knock** | 1 (in 37-in-1 kits) | DO: any DIO (some also AO) | 3.3–5 V | none | `digital-in` (hall/reed/tilt/flame), `counter` for pulses | Yes |
| **HX711 load cell amp** | 2 | DT: any DIO in, SCK: any DIO out (not I²C despite the names) | 2.6–5.5 V | HX711 (bogde) | `force` | Bit-banged, timing-sensitive; better on Arduino |

**Gotchas**: DHT reads take ~5 ms (DHT22 ~25 ms) with tight timing, and the sensor must not be read more
often than about once per 1 s (DHT11) / 2 s (DHT22). The DHT library disables interrupts briefly.

### 2.4 Input

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **Push button / tactile switch / toggle** | 1 | any DIO in with `INPUT_PULLUP`, button to GND | — | none, or Bounce2 | `button` | Yes (internal pull-ups) |
| **Bumper / micro (limit) switch / end-stop** | 3 (robots: 1) | any DIO in, pull-up; an interrupt pin if it must stop motors instantly | — | none | ★ `obstacle` (bumper) / `digital-in` (limit switch) | Yes, but a stop should live on the MCU |
| **TTP223 capacitive touch** | 2 | Out: any DIO in | 2–5.5 V | none | `button` | Yes |
| **Potentiometer / slide pot** | 1 | Wiper: analog in; ends to 5 V and GND | — | none | `analog-in` | Needs an ADC |
| **Analog thumb joystick** (KY-023) | 1 | VRx, VRy: analog in; SW: any DIO in with pull-up | 5 V (or 3.3 V) | none | `rc-input` (axes + button) | Needs an ADC |
| **4×4 / 3×4 matrix keypad** | 1 | 4 rows + 4 cols: 8 DIO (or one I²C expander) | — | Keypad (Stanley/Brevig) | `keypad` | Yes |
| **IR receiver** VS1838B / TSOP38238 + remote | 1 | Out: any DIO in (an interrupt/timer pin depending on library) | 2.7–5.5 V | IRremote (MIT) | `ir-remote` | Yes, via the kernel `gpio-ir` overlay |
| **RC receiver, PWM** (FS-iA6, any hobby RX) | 3 | 1 DIO in per channel, interrupt or pin-change pins | 5 V from BEC; 3.3 V signal | none / EnableInterrupt | `rc-input` | Timing-critical: Arduino |
| **RC receiver, PPM / iBUS / SBUS / CRSF** | 3 | PPM: one interrupt pin. iBUS/CRSF: a hardware serial RX. **SBUS is inverted 100 000 baud 8E2**: needs a hardware inverter on AVR | 5 V | PPMReader, IBusBM, sbus libraries | `rc-input` | iBUS/CRSF on a Pi UART is fine |
| **nRF24L01+** (and PA+LNA version) as a radio joystick link | 2 | SPI (MOSI/MISO/SCK) + CE: any DIO + CSN: any DIO (+ IRQ optional) | **3.3 V only supply**, 5 V-tolerant logic; PA+LNA draws ~100 mA+ peaks | RF24 (nRF24 org, GPL-2.0) | transport → feeds `rc-input` | Yes (SPI) |
| **HC-05 / HC-06** Bluetooth SPP | 1 | TX → board RX, RX ← board TX **through a 1 k/2 k divider**; a hardware serial port preferred; EN/KEY for AT mode | VCC 3.6–6 V (module regulator), logic 3.3 V | none | transport, not a capability | Not needed (Pi has Bluetooth) |

**Gotchas**
- nRF24 boards are notorious for brown-outs: a 10–100 µF capacitor across VCC/GND at the module is the
  standard fix, and the Nano's 3.3 V pin (from the USB chip) cannot feed a PA+LNA version
  *(general knowledge, widely repeated on the forum)*.
- HC-05 RX is not 5 V-tolerant: Arduino TX → 1 kΩ → HC-05 RX → 2 kΩ → GND. HC-05 TX can go straight
  to the Arduino RX. On an Uno it must not share D0/D1 with the USB link to Faber.

### 2.5 Output: lights, sound, relays, displays

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **LED** (with 220–1 kΩ resistor) | 1 | any DIO out; a PWM pin to dim | ~10–20 mA per pin max (AVR: 20 mA recommended, 40 mA absolute) | none | `digital-out` / `pwm-out` | Yes (16 mA pad limit, few mA recommended) |
| **RGB LED** (common cathode or anode) | 1 | 3 PWM pins | as LED | none | `light` (1 pixel) | Yes (software PWM) |
| **WS2812B / SK6812 / NeoPixel strip, ring, matrix** | 2 (kits) / 1 (popularity: FastLED is top-3 by stars) | DIN: any DIO out, **300–500 Ω series resistor** at the strip | 5 V, up to ~60 mA per pixel at full white *(general knowledge)*; **own supply**; **500–1000 µF capacitor** across the strip's supply; shared GND | Adafruit_NeoPixel (LGPL-3.0), FastLED (MIT) | `light` | 3.3 V data needs a level shifter (74AHCT125). Pi 5: the old rpi_ws281x approach does not work on RP1; SPI- or PIO-based drivers do *(verify)* |
| **Active buzzer** | 1 | any DIO out (fixed tone) | 5 V, ~30 mA | none | `tone` (fixed frequency) or `digital-out` | Yes |
| **Passive buzzer / piezo / small speaker** | 1 | `tone()` pin: any DIO, but `tone()` uses Timer2 | — | none | `tone` | Software PWM, poor |
| **Relay module** (1/2/4/8 ch, SRD-05VDC) | 1 | IN: any DIO out; many are **active low** | 5 V coil ~70 mA per channel; optocoupled boards have a JD-VCC jumper for a separate coil supply | none | `digital-out` (active-low setting) | 5 V coil modules often don't switch reliably from 3.3 V logic |
| **MOSFET module** (IRF520 board, logic-level AO3400 / IRLZ44N) | 2 | any PWM pin | load supply separate | none | `pwm-out` | IRF520 boards are not logic-level; poor at 5 V, useless at 3.3 V *(general knowledge)* |
| **16×2 / 20×4 LCD (HD44780) with PCF8574 I²C backpack** | 1 | I²C: SDA/SCL at **0x27** (PCF8574T) or **0x3F** (PCF8574AT). Parallel version: RS, E, D4–D7 = 6 DIO | 5 V, backlight ~20–100 mA | LiquidCrystal_I2C (several forks), hd44780 (Bill Perry) | `text-display` | I²C works; the LCD wants 5 V, but PCF8574 at 5 V pulls SDA/SCL to 5 V: remove pull-ups or level-shift |
| **SSD1306 OLED** 128×64 / 128×32 (0.96″), SH1106 (1.3″) | 1 | I²C 0x3C (or 0x3D); SPI versions too | 3.3–5 V modules | Adafruit SSD1306 + GFX (BSD), U8g2 (BSD-2) | `pixel-display` | Yes |
| **TM1637 4-digit 7-segment** | 1 | CLK, DIO: any 2 DIO (not real I²C) | 3.3–5 V | TM1637Display (avishorp) | `text-display` | Yes (bit-banged) |
| **MAX7219** 8×8 matrix / 8-digit 7-seg | 2 | DIN, CLK, CS: any 3 DIO (or hardware SPI) | 5 V, up to several hundred mA per 8×8 | LedControl, MD_MAX72XX / MD_Parola | `pixel-display` (matrix) / `text-display` (digits) | Logic is 5 V (3.3 V marginal) |
| **74HC595 shift register** | 1 (kits) | DS, SHCP, STCP: 3 DIO | — | `shiftOut()` | none: a pin expander (§4.3) | Yes |
| **TFT ILI9341 / ST7735 / ST7789** | 2 | SPI + DC, RST, CS | 3.3 V logic (some boards 5 V-tolerant) | Adafruit GFX + driver, TFT_eSPI (ESP-oriented) | `pixel-display` | Yes |

**Gotchas**
- **NeoPixels disable interrupts while sending**: about 30 µs per pixel (24 bits at 800 kHz), so 60
  pixels ≈ 1.8 ms and 300 pixels ≈ 9 ms with interrupts off. At 115200 baud a byte arrives every
  ~87 µs and the AVR's hardware UART buffers only a couple of bytes, so **bytes from Faber are lost**
  during a long `show()`, `millis()` drifts, and encoder edges are missed *(timing from the WS2812
  protocol, general knowledge)*. SENNEP's choice to drive the strip from a separate Nano is exactly
  right, and guided mode should recommend it: warn at about 30+ pixels on a board that also runs
  FaberLink with motors or encoders, and refresh the strip only on change.
- Powering a strip from the Arduino's 5 V pin is the classic beginner failure (USB gives ~500 mA in
  total).
- The 74HC595, MAX7219 and TM1637 are not I²C even though some sellers say "two-wire"; the allocator
  must not put them on the I²C pins by default.

### 2.6 Motors and drivers

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **L298N** dual H-bridge board | 1 | Per motor: **ENA: PWM** (jumper removed), **IN1, IN2: DIO** | Motor supply 5–35 V (~2 V lost in the bipolar bridge); the on-board 5 V regulator only up to 12 V input (jumper) | none | `motor` (dc) | GPIO OK; PWM via hardware or software |
| **L293D** chip / motor shield v1 (with 74HC595) | 1 (official kit) | EN: PWM, IN1, IN2 per motor | 4.5–36 V, 600 mA per channel | AFMotor (Adafruit, retired) for the shield | `motor` (dc) | Yes |
| **TB6612FNG** | 2 (robots: 1) | Per motor: **PWMA: PWM, AIN1, AIN2: DIO**; **STBY**: DIO (or tie high) | VM 2.5–13.5 V, 1.2 A continuous; logic 2.7–5.5 V | SparkFun TB6612 | `motor` (dc) | Yes, 3.3 V logic OK |
| **L9110S / HG7881** | 2 | **IA, IB** per motor: one PWM + one DIO (or two PWM for both directions) | 2.5–12 V, ~800 mA | none | `motor` (dc) | Yes |
| **DRV8833** (and DRV8835) | 2 | **AIN1, AIN2: both PWM** for speed in both directions (or PWM + DIO); nSLEEP | 2.7–10.8 V, ~1.5 A | none | `motor` (dc) | Yes |
| **BTS7960 (IBT-2)** 43 A | 3 (robots) | **RPWM, LPWM: both PWM**; R_EN, L_EN: DIO (or tied high); R_IS/L_IS: current sense to analog | 5.5–27 V, high current | none | `motor` (dc, `current-a` via IS) | Yes, 3.3 V logic is usually accepted *(verify)* |
| **Cytron MD10C / MDD10A** | 3 | **PWM + DIR** (the shape `FaberMotor` takes today) | 5–30 V | none | `motor` (dc) | Yes |
| **SG90 / MG90S micro servo** | 1 | Signal: **any DIO** with the Servo library (it uses a timer, not the pin's PWM) | 4.8–6 V; ~100–250 mA moving, **~650 mA+ stall** for SG90-class *(typical figures, verify)* | Servo (Arduino, LGPL-2.1) | `servo` | Hardware PWM on GPIO12/13/18/19; software PWM jitters |
| **MG995 / MG996R** standard servo | 2 | as above | 4.8–7.2 V, **stall ~2.5 A** *(typical, verify)*: own supply always | Servo | `servo` | as above |
| **Continuous-rotation servo** (FS90R) | 2 | as above | as above | Servo | `motor` (continuous-servo) | as above |
| **PCA9685** 16-ch PWM/servo board | 2 | SDA/SCL, default **0x40** (6 address pins: 0x40–0x7F) | V+ terminal for servos, own supply | Adafruit PWM Servo Driver | many `servo` / `pwm-out` | Yes, the standard way to drive servos from a Pi |
| **28BYJ-48 + ULN2003** | 1 | IN1–IN4: 4 DIO | 5 V, ~240 mA *(typical)* | Stepper (Arduino, note coil order 1-3-2-4), AccelStepper (GPL-3.0) | `motor` (stepper) | Yes |
| **NEMA 17 + A4988 / DRV8825 / TMC2208** | 2 | STEP: DIO (timing-critical), DIR: DIO, EN: DIO (active low), MS1–3 (microsteps) | 8–35 V (A4988) / 8.2–45 V (DRV8825); **100 µF across VMOT**; set current with the pot | AccelStepper | `motor` (stepper) | Step pulses from Linux jitter: Arduino |
| **Brushless ESC** (car/drone) | 3 | Signal: servo-style PWM pin | Battery; ESC BEC may supply 5 V (don't connect two BECs) | Servo | `motor` (bldc) | as servo |

**Gotchas**
- **Driver pin shapes differ**, and that matters for FaberLink (§3): `FaberMotor` today is PWM + DIR,
  which fits Cytron-style drivers directly but none of the most common hobby drivers. L298N/L293D/
  TB6612 need **EN(PWM) + IN1 + IN2**; L9110/DRV8833/BTS7960 need **two PWM inputs** (IN1/IN2 or
  RPWM/LPWM). *Built 2026-10-16: `FaberMotor` takes all three shapes ([faberlink.md](../faberlink.md) §7).*
- L298N: users leave the ENA/ENB jumpers on (full speed, no PWM), forget the common ground between the
  motor supply and the Arduino, or feed >12 V with the 5 V jumper fitted.
- Servos from the Arduino 5 V pin: one SG90 idling may work; two moving, or any MG996R, brown out the
  board (it resets, USB drops). Guided mode should say "own 5–6 V supply, common ground" for ≥ 1
  servo under load.
- DC motor drivers want a PWM frequency the motor tolerates; Uno pins 5/6 run at ~980 Hz, the others
  at ~490 Hz *(Arduino docs, general knowledge)*; Timer0 pins (5/6 Uno, 4/13 Mega) must not be
  retimed because `millis()` uses Timer0.

### 2.7 Power

| Part (variants) | Tier | Pins | Power | Library | Cap. | Pi |
|---|---|---|---|---|---|---|
| **Battery voltage divider** | 1 (robots) | Divider midpoint: analog in | Keep the pin below AREF at full charge | none | `battery` — `FaberBattery` exists | Needs an ADC |
| **INA219** (26 V, 12-bit) / **INA226** (36 V, 16-bit) | 2 | SDA/SCL, **0x40** default (A0/A1 give 16 addresses 0x40–0x4F) | 3–5.5 V logic; shunt on the load path (high or low side) | Adafruit INA219, INA226 (Rob Tillaart) | `power-meter`, and `battery.amps` | Yes |
| **ACS712** hall current sensor (5/20/30 A) | 2 | OUT: analog in, centred at VCC/2 | 5 V | none | `power-meter` (amps only) | Needs an ADC and a 5 V → 3.3 V scale |
| **ADS1115 / MCP3008** ADC | 2 (Pi: 1) | ADS1115 I²C 0x48–0x4B; MCP3008 SPI + CS | 2–5.5 V | Adafruit ADS1X15 | none: a pin provider (§4.3) | **The Pi needs one for every analog part above** |

**Gotcha**: **INA219/INA226 and PCA9685 both default to 0x40**: a robot with a current sensor and a
servo board has a clash out of the box.

### 2.8 Other common parts (tier 2–3, map onto existing capabilities)

| Part | Pins | Cap. | Note |
|---|---|---|---|
| **RC522 RFID** (MFRC522) | SPI + SS + RST; **3.3 V only** (logic is 5 V-tolerant per many reports, not per datasheet) | `tag-reader` | MFRC522 library is top-10 by forks |
| **DS3231 / DS1307 RTC** | I²C 0x68 (+ AT24C32 EEPROM 0x57 on DS3231 boards) | none (by design: time belongs to the core); the DS3231's temperature could be a `temperature` | Clashes with IMUs at 0x68 |
| **TCS34725 colour** | I²C 0x29 fixed | `color-sensor` | Clashes with VL53L0X/VL53L1X at 0x29 until the ToF is re-addressed |
| **APDS-9960** gesture/colour/proximity | I²C 0x39 | `color-sensor` + `presence` | |
| **MAX30102** pulse oximeter | I²C 0x57 | skipped: biometric (capabilities.md "not capabilities") | Same 0x57 as the DS3231 module's EEPROM |
| **TCRT5000 single line/reflectance** | AO analog, DO digital | ★ `line-sensor` (1 channel) | |
| **QTR-8A/8RC / 5-channel line arrays** | 8 analog (A) or 8 DIO with timing (RC), an emitter-enable pin | ★ `line-sensor` | Johnny-Five has a ReflectanceArray class |
| **PCF8574 / MCP23017** I/O expanders | I²C 0x20–0x27 (PCF8574A 0x38–0x3F) | none: pin provider | MCP23017 range collides with LCD backpacks at 0x27 |
| **CD74HC4067** 16-ch mux | 4 select DIO + 1 analog | none: pin provider | ESPHome supports it |

---

## 3. Recommended waves for guided mode

The ranking multiplies popularity by relevance to SENNEP-class robots (DC motors, encoders, IMU,
close-range distance, bumpers, battery, LED strip, RF joystick).

### First wave (12 templates)

| # | Template | Covers | FaberLink class | Capability |
|---|---|---|---|---|
| 1 | Ultrasonic distance | HC-SR04, HC-SR04P, JSN-SR04T | `FaberUltrasonic` (exists; make echo non-blocking later) | `distance` ✔ |
| 2 | Analog IR distance | Sharp GP2Y0A21/02/41 | `FaberDistance` (exists) | `distance` ✔ |
| 3 | Battery voltage | divider | `FaberBattery` (exists) | `battery` ✔ |
| 4 | DC motor + driver | L298N, L293D, TB6612, L9110, DRV8833, BTS7960, Cytron | **extend `FaberMotor`** with a driver shape: `PWM_DIR` (today), `EN_IN1_IN2`, `IN1_IN2_PWM` (two PWM pins) | `motor` ✔ |
| 5 | Quadrature encoder | wheel encoders, KY-040 knob | **new `FaberEncoder`**, plus an optional encoder on `FaberMotor` for measured velocity | `encoder` ✔ |
| 6 | IMU | MPU6050 first (it is the kit IMU), BNO055 / ICM-20948 next | **new `FaberImu`** (in an optional header, see below) | `imu` ✔ |
| 7 | Laser ToF distance | VL53L0X, VL53L1X | **new `FaberToF`**, which owns the XSHUT addressing at every boot | `distance` ✔ |
| 8 | Button / switch / bumper | tactile button, TTP223, micro switch, limit switch | **new `FaberButton`** (debounce, active-low, optional "stop motors on press") | `button` ✔ / ★ `obstacle` |
| 9 | Digital and PWM output | LED, relay, MOSFET, active buzzer | **new `FaberOutput`** (digital or PWM, active-low, safe level) | `digital-out` ✔ / `pwm-out` ✔ |
| 10 | Servo | SG90, MG90S, MG996R | **new `FaberServo`** | `servo` ✔ |
| 11 | Addressable LED strip | WS2812B, SK6812 | **new `FaberPixels`** (optional header) | `light` ✔ |
| 12 | Analog input / joystick | potentiometer, LDR, thumb joystick | **new `FaberAnalogIn`**, and a joystick template = 2 analog + 1 button | `analog-in` ✔ / `rc-input` ✔ |

Eight new library classes; **no new capability is strictly needed for the first wave** (only the
bumper reads better as `obstacle`). The IMU, ToF and pixel classes depend on third-party libraries,
so they should live in **optional headers** (`FaberImu.h`, `FaberPixels.h`, …) so that plain
`#include <FaberLink.h>` still compiles with no other library installed, and guided mode installs the
dependency when it adds the part.

### Second wave

DHT11/DHT22 and DS18B20 (`temperature`/`humidity`); BMP280/BME280; I²C 16×2 LCD and SSD1306 OLED
(`text-display`, `pixel-display`); passive buzzer (`tone`); PIR (`presence`); IR receiver
(`ir-remote`); TCRT5000 and QTR arrays (★ `line-sensor`); INA219/INA226 (`power-meter`); steppers:
28BYJ-48 + ULN2003 and A4988/DRV8825 (`motor` stepper); nRF24L01 joystick link and RC receivers
PWM/PPM/iBUS (`rc-input`); TF-Luna/TFmini (`distance`); TM1637; 4×4 keypad; PCA9685 servo board;
HC-05 as a transport (a wiring template, no capability).

### Third wave

MAX7219, RC522, HX711, MQ-x gas, soil moisture, sound sensor, GPS NEO-6M (better on the Pi),
HMC5883L/QMC5883L, BNO085, brushless ESC, BTS7960 current sense, I/O expanders and ADCs as pin
providers, DS3231 (as a time source), TCS34725, BH1750, SCD40, PMS5003, the 37-in-1 digital modules
(hall, reed, tilt, flame, vibration: one generic "digital sensor module" template covers all of them).

---

## 4. Capability gaps and mappings

### 4.1 Missing capabilities

| Proposed | Signal / info | Rationale |
|---|---|---|
| ★ **`line-sensor@1`** | `values[]` 0–1 (reflectance per channel), `position?` −1…1 (line centre), `on-line` bool; info: channel count, spacing-m, mount | Line followers are the most common beginner robot after obstacle avoiders; `analog-in` ×8 loses the array's geometry. Johnny-Five's ReflectanceArray is the same idea. |
| ★ **`obstacle@1`** | `detected` bool (**E**), info: kind (bumper / infrared / limit), mount, trigger-distance-m? | Bumpers and FC-51/E18-D80NK obstacle sensors are binary spatial sensors. As `button`, an app looking for "a button to start" would pick a bumper; as `distance` they have no metres. The top view can show them. |

That is all. The existing list of 41 capabilities already covers the other ~55 parts here, which
says the catalogue was well chosen.

### 4.2 Parts that should map to an existing generic capability, not a new one

- Analog joystick → **`rc-input`** (ROS Joy covers axes + buttons whether by wire or by radio).
- Potentiometer, flex sensor, water-level strip → `analog-in` (or `measurement` with a unit).
- LDR / TEMT6000 → `illuminance`, but `illuminance` needs a **`calibrated` info flag** like
  `sound-level` and `concentration` have, because an LDR gives relative light only.
- Active buzzer → `tone` with a fixed frequency (info), or `digital-out`.
- Relay, solenoid, MOSFET → `digital-out` / `pwm-out` (as capabilities.md already says).
- Hall, reed, tilt, flame, vibration digital modules → `digital-in`; pulse outputs → `counter`.
- Continuous servo → `motor` (continuous-servo); stepper → `motor` (stepper); ESC → `motor` (bldc).
- ACS712/INA219 → `power-meter`; when it measures the battery, also feed `battery.amps`.
- Limit switches/end-stops on an axis → `digital-in` (or the stepper's own homing, later).
- nRF24, HC-05, ESP-01 → transports, not capabilities (as decided).
- DS3231 → not a capability (as decided), but its temperature can be a `temperature`.

### 4.3 Not capabilities, but the allocator needs them: pin providers

PCA9685, PCF8574/MCP23017, 74HC595, CD74HC4067, ADS1115/MCP3008 don't measure anything; they **add
pins** (PWM, digital, analog). Two options: (a) leave them out of guided mode, or (b) let a part
template declare that it provides a small pin map of its own, which the allocator then offers
("servo on PCA9685 channel 3"). Option (b) is what makes the **Pi** usable for analog parts at all
(the Pi has no ADC) and lets one Arduino drive 16 servos. Recommended: design the pin-map format so a
part can contribute a pin map, build it in the third wave.

---

## 5. Pin-map facts the allocator must know

### Uno / Nano (ATmega328P)
- **External interrupts**: D2 (INT0), D3 (INT1) only. Pin-change interrupts exist on every pin, in
  three port groups (D0–D7, D8–D13, A0–A5), one vector per group.
- **PWM and timers**: Timer0 → D5, D6 (`millis()`, ~980 Hz); Timer1 → D9, D10 (~490 Hz, **Servo
  library**); Timer2 → D3, D11 (~490 Hz, **`tone()`**).
- **Servo** disables PWM on D9/D10 as soon as the library is used, whichever pins the servos are on.
- **I²C**: A4 = SDA, A5 = SCL. The R3's SDA/SCL header pins near AREF are the **same signals**:
  aliases, not extra pins.
- **SPI**: D10 SS, D11 MOSI, D12 MISO, D13 SCK (also on ICSP). D10 must stay an output in SPI master
  mode even if unused as SS *(general knowledge)*.
- **Serial**: D0/D1 = USB link to Faber: reserved. SoftwareSerial works on any pin but receives on only
  one port at a time, disables interrupts per byte, and is unreliable at high baud;
  **AltSoftSerial** is better but fixes the pins (TX D9, RX D8) and takes Timer1, which conflicts with
  Servo *(general knowledge, verify)*.
- **Nano**: A6/A7 are **analog only** (no digital I/O, no pull-up). The Nano's 3.3 V pin comes from the
  USB-serial chip and supplies very little.
- Pin current: 20 mA recommended, 40 mA absolute per pin; the chip has a total limit near 200 mA
  *(ATmega328P datasheet, general knowledge)*. USB 5 V is ~500 mA for everything.

### Mega 2560
- **External interrupts**: 2, 3, 18, 19, 20, 21. **18/19 = Serial1 TX1/RX1, 20/21 = I²C SDA/SCL**: a
  pin is lost to one or the other.
- **Pin-change interrupts** only on 10, 11, 12, 13, 50, 51, 52, 53, A8–A15 (D62–D69).
- **PWM and timers**: Timer0 → 4, 13 (`millis()`); Timer1 → 11, 12; Timer2 → 9, 10 (**`tone()`**);
  Timer3 → 2, 3, 5; Timer4 → 6, 7, 8; Timer5 → 44, 45, 46.
- **Servo on the Mega takes Timer5 first** (the library's order is Timer5, Timer1, Timer3, Timer4, 12
  servos per timer), so **the first servo kills PWM on 44, 45, 46**; servos 13–24 also take 11, 12.
  Arduino's reference only mentions 11/12, which is misleading. **Correction for wiring.md §4**: the
  Mega rule should say "44–46 from the first servo; 11/12 after 12 servos".
- **I²C**: 20 SDA, 21 SCL (aliased again on the SDA/SCL header). The Mega R3 has 10 kΩ pull-ups on
  20/21 on the board *(general knowledge, verify)*, so they read high as plain inputs.
- **SPI**: 50 MISO, 51 MOSI, 52 SCK, 53 SS (53 must stay an output in master mode).
- **Serial**: 0/1 (USB, reserved), Serial1 19 RX/18 TX, Serial2 17 RX/16 TX, Serial3 15 RX/14 TX.
  Use these for GPS, TF-Luna, HC-05, the Pi link, never SoftwareSerial on a Mega. (SoftwareSerial RX
  works only on the pin-change pins listed above.)
- 16 analog inputs A0–A15, all usable as digital pins.

### Raspberry Pi 5 (40-pin header)
- **3.3 V logic, not 5 V-tolerant.** Pads are designed for 16 mA; the docs warn that the total current
  can collapse the 3.3 V supply if every pin is loaded.
- **No ADC.** Every analog part needs an ADS1115/MCP3008 (I²C/SPI).
- **Hardware PWM**: GPIO12, 13, 18, 19 (RP1 PWM0). On the Pi 5, channels 0/1 drive GPIO12/13 and 2/3
  drive GPIO18/19, and the sysfs PWM chip index differs from older Pis (rpi-hardware-pwm docs).
  Software PWM elsewhere jitters: fine for LEDs, not for servos.
- **I²C1**: GPIO2 SDA / GPIO3 SCL with **fixed pull-ups** (they can't be used as plain inputs without
  them). GPIO0/1 (ID_SD/ID_SC) are reserved for HAT ID EEPROMs.
- **SPI0**: GPIO10 MOSI, 9 MISO, 11 SCLK, 8 CE0, 7 CE1. **SPI1**: GPIO20 MOSI, 19 MISO, 21 SCLK,
  18/17/16 CE0–2 (note: overlaps the PWM pins 18/19).
- **UARTs**: uart0 on GPIO14/15; uart1–uart4 via `uartN-pi5` overlays on GPIO0/1, 4/5, 8/9, 12/13
  (fixed pairs on the Pi 5, cannot be remapped). The serial console moved to a separate debug
  connector on the Pi 5. Note the overlaps: uart1 vs the HAT EEPROM pins, uart3 vs SPI0, uart4 vs PWM
  12/13.
- GPIO on the Pi 5 goes through the **RP1** chip: older libraries that poke Broadcom registers
  (RPi.GPIO, pigpio, rpi_ws281x) don't work; character-device GPIO (libgpiod) does *(general
  knowledge)*. Linux is not real-time: ultrasonic echo timing, wheel encoders, step pulses, DHT
  bit-banging and PWM/PPM RC decoding belong on the Arduino.

### Cross-board rules for the allocator
1. **I²C address table** with known clashes: 0x68 (MPU6050/MPU9250/ICM-20948 vs DS3231/DS1307),
   0x29 (VL53L0X/VL53L1X vs TCS34725/TSL2591, and BNO055 alt), 0x40 (INA219/INA226 vs PCA9685),
   0x27/0x20–0x27 (LCD backpack vs MCP23017/PCF8574), 0x57 (DS3231 EEPROM vs MAX30102), 0x3C (all
   SSD1306), 0x76/0x77 (BMP/BME280 vs BMP180 at 0x77). Each template lists its possible addresses and
   how to change them (pin strap, solder jumper, software like the VL53L0X).
2. **Voltage table per part** (5 V / 3.3 V / both, 5 V-tolerant or not), checked against the board:
   a 3.3 V-only part on a 5 V board gets a level-shifter warning; a 5 V-output part (HC-SR04 echo) on
   a Pi gets a divider.
3. **Power budget**: sum the typical current of parts powered from the board's 5 V/3.3 V pins and
   warn above the USB limit; servos, motors, strips and MQ heaters are always "own supply".
4. **Timer resources** as allocatable things, not just pins: Servo, `tone()`, AltSoftSerial,
   NeoPixel (interrupts off), NewPing timer mode each claim a timer or the interrupt budget.
5. **Interrupt pins are scarce** (2 on an Uno, ~4 really free on a busy Mega): suggest them first to
   encoders, then RC PWM inputs, then ultrasonic echo; never suggest them for LEDs.

---

## Sources

Kits
- Elegoo Mega 2560 Most Complete Starter Kit: https://us.elegoo.com/collections/arduino-kits/products/elegoo-mega-2560-the-most-complete-starter-kit
- Elegoo UNO Super Starter Kit review (contents): https://the-diy-life.com/elegoo-uno-project-super-starter-kit-unboxing-review/
- Elegoo Smart Robot Car Kit V4.0: https://us.elegoo.com/products/elegoo-smart-robot-car-kit-v-4-0 and hardware notes: https://cs1.txwes.edu/~rnunez/hardWare.html
- Elegoo gyro + arm tutorial (MPU6050, PCA9685): https://www.elegoo.com/blogs/learn/tutorial-add-mechanical-arm-on-a-gyro-control-smart-robot-car
- Arduino Starter Kit K000007 contents: https://core-electronics.com.au/arduino-starter-kit-9607.html and https://rapidonline.com/arduino-k000007-starter-kit-including-uno-board-563530
- Keyestudio 37-in-1 Sensor Kit V2.0 module list: https://docs.keyestudio.com/projects/KS0399-KS0400-KS0401/en/latest/KS0399,0400,0401.html
- SunFounder Universal Maker Sensor Kit: https://docs.sunfounder.com/projects/umsk/en/latest/ and https://www.elektor.com/products/universal-maker-sensor-kit-for-raspberry-pi-pico-w-arduino-esp32
- Freenove Ultimate Starter Kit for Raspberry Pi (FNK0020): https://manuals.plus/asin/B06W54L7B5

Tools and libraries
- Wokwi supported hardware: https://docs.wokwi.com/getting-started/supported-hardware
- ESPHome components: https://esphome.io/components/
- Johnny-Five API, Proximity and IMU: https://johnny-five.io/api/ , https://johnny-five.io/api/proximity/ , https://johnny-five.io/api/imu/
- Arduino library stars/forks: https://www.arduinolibraries.info/

Pin and part facts
- Servo library reference: https://www.arduino.cc/en/Reference/Servo ; timer order: https://github.com/arduino-libraries/Servo/blob/master/src/avr/ServoTimers.h
- Mega timer/PWM map: https://forum.arduino.cc/t/timer-pwm-conflicts-on-the-mega-2560/296268 and https://forum.arduino.cc/t/timers-on-the-arduino-mega-2560/57523
- Raspberry Pi GPIO documentation: https://www.raspberrypi.com/documentation/computers/raspberry-pi.html
- Pi 5 hardware PWM channels: https://pypi.org/project/rpi-hardware-pwm
- Pi 5 UART overlays: https://github.com/Felipegalind0/RPI5.pinout and https://pinout.xyz/pinout/uart
- NeoPixel best practices: https://learn.adafruit.com/adafruit-neopixel-uberguide/best-practices
- VL53L0X multiple sensors / XSHUT: https://learn.adafruit.com/adafruit-vl53l0x-micro-lidar-distance-sensor-breakout/arduino-code and https://forum.pololu.com/t/vl53l0x-multiple-sensors-adjust-ic2-address/16808
- HC-05 RX divider: https://www.martyncurrey.com/?p=1294
