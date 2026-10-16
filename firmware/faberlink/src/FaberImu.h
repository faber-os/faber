// FaberImu: an MPU6050 (the GY-521 board in most starter kits) as imu@1, on I2C. An optional
// header, so only a sketch with an IMU carries the Wire library.
//
//   #include <FaberLink.h>
//   #include <FaberImu.h>
//
//   FaberImu imu("imu");                                // at 0x68 (AD0 low or open)
//   FaberImu imu2("imu2", faberlink::IMU_ADDRESS_69);   // AD0 to VCC: 0x69
//
// Wiring: VCC -> 5V (the GY-521 has its own 3.3 V regulator and pull-ups to 3.3 V), GND -> GND,
// SDA -> the board's SDA (A4 on an Uno, 20 on a Mega), SCL -> SCL (A5, 21). A DS3231 or DS1307
// clock module is also at 0x68: with one on the bus, put the IMU's AD0 to VCC.
//
// It sends rates (rad/s) and accelerations (m/s^2, gravity included) along the chip's own axes,
// 20 times a second; its mount in Faber says how the chip sits on the machine. No orientation:
// the chip's motion processor (DMP) isn't used. A missing or silent IMU sends nothing, and never
// blocks the board: every transfer has a 3 ms time-out.
//
// The registers and scales are from InvenSense's "MPU-6000 and MPU-6050 Register Map and
// Descriptions" (RM-MPU-6000A-00, revision 4.2): PWR_MGMT_1 0x6B (0x01 wakes it with the X gyro's
// clock), CONFIG 0x1A (DLPF_CFG 3: 44 Hz accelerometer, 42 Hz gyro bandwidth), GYRO_CONFIG 0x1B
// (FS_SEL 0: +-250 deg/s, 131 LSB per deg/s), ACCEL_CONFIG 0x1C (AFS_SEL 0: +-2 g, 16384 LSB
// per g), and ACCEL_XOUT_H 0x3B, the start of 14 bytes: acceleration x, y, z, the temperature,
// then the rates x, y, z, each a big-endian int16. WHO_AM_I 0x75 reads 0x68 on an MPU6050; boards
// sold as one sometimes carry another chip (whoAmI() says which).

#ifndef FABER_IMU_H
#define FABER_IMU_H

#include <Wire.h>

#include "FaberLink.h"

namespace faberlink {

// The MPU6050's I2C address, set by its AD0 pin.
enum ImuAddress : uint8_t {
  IMU_ADDRESS_68 = 0x68,  // AD0 low or open (the default)
  IMU_ADDRESS_69 = 0x69,  // AD0 to VCC
};

}  // namespace faberlink

class FaberImu : public faberlink::Imu {
 public:
  explicit FaberImu(const char* name, faberlink::ImuAddress address = faberlink::IMU_ADDRESS_68, float rateHz = 20)
      : faberlink::Imu(name, rateHz > 0 && rateHz <= 1000 ? static_cast<uint16_t>(1000.0f / rateHz + 0.5f) : 50),
        address_(address),
        whoAmI_(0) {
    faberlink::autoAdd(*this);
  }

  void begin() override {
    Wire.begin();
#if defined(ARDUINO_ARCH_AVR)
    // AVR core 1.8.3 and later: a stuck bus resets instead of hanging loop() (and FaberLink).
    Wire.setWireTimeout(3000, true);
#endif
    wake();
  }

  // What the chip's WHO_AM_I register said at the last wake-up (0x68 for an MPU6050), 0 when it
  // didn't answer.
  uint8_t whoAmI() const { return whoAmI_; }

 protected:
  uint8_t writePins(uint8_t* out) const override {
#if defined(PIN_WIRE_SDA) && defined(PIN_WIRE_SCL)
    out[0] = faberlink::ROLE_SDA;
    out[1] = PIN_WIRE_SDA;
    out[2] = faberlink::ROLE_SCL;
    out[3] = PIN_WIRE_SCL;
    return 4;
#else
    (void)out;
    return 0;
#endif
  }

  bool measure(float angularVelocity[3], float linearAcceleration[3]) override {
    if (whoAmI_ == 0) {
      wake();  // plugged in after the board started, or it lost power: try again
      if (whoAmI_ == 0) return false;
    }
    uint8_t b[14];
    if (!read(0x3B, b, sizeof(b))) {
      whoAmI_ = 0;
      return false;
    }
    const float mPerS2 = 9.80665f / 16384.0f;               // +-2 g; standard gravity per g
    const float radPerS = 3.14159265f / 180.0f / 131.0f;    // +-250 deg/s
    for (uint8_t i = 0; i < 3; i++) {
      linearAcceleration[i] = be16(b + 2 * i) * mPerS2;
      angularVelocity[i] = be16(b + 8 + 2 * i) * radPerS;
    }
    return true;
  }

 private:
  static int16_t be16(const uint8_t* p) { return static_cast<int16_t>((static_cast<uint16_t>(p[0]) << 8) | p[1]); }

  bool writeRegister(uint8_t reg, uint8_t value) {
    Wire.beginTransmission(address_);
    Wire.write(reg);
    Wire.write(value);
    return Wire.endTransmission() == 0;
  }

  bool read(uint8_t reg, uint8_t* out, uint8_t n) {
    Wire.beginTransmission(address_);
    Wire.write(reg);
    if (Wire.endTransmission(false) != 0) return false;
    if (Wire.requestFrom(address_, n) != n) return false;
    for (uint8_t i = 0; i < n; i++) out[i] = static_cast<uint8_t>(Wire.read());
    return true;
  }

  // Wakes it (it starts asleep) and sets its ranges, its filter and its clock.
  void wake() {
    uint8_t who = 0;
    const bool ok = writeRegister(0x6B, 0x01) && writeRegister(0x1A, 0x03) && writeRegister(0x1B, 0x00) &&
                    writeRegister(0x1C, 0x00) && read(0x75, &who, 1);
    whoAmI_ = ok ? (who == 0 ? 0xFF : who) : 0;
  }

  uint8_t address_;
  uint8_t whoAmI_;
};

#endif
