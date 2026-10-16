// Mpu6050: an MPU6050 IMU (a GY-521 board) on I2C, read by Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), a GY-521 board, four jumper wires, and a USB
// cable to the computer running Faber. The Wire library comes with the board's core.
//
// Wiring (the GY-521 has its own 3.3 V regulator, so it takes the board's 5V):
//   GY-521 VCC -> 5V, GND -> GND
//   GY-521 SDA -> SDA (A4 on an Uno or Nano, 20 on a Mega)
//   GY-521 SCL -> SCL (A5 on an Uno or Nano, 21 on a Mega)
//   GY-521 AD0 -> leave open (address 0x68), or to VCC for 0x69
//
// What you should see: add the board in Faber (say as "arduino"). "arduino.imu" is an IMU: lying
// flat its viewer shows about 9.81 m/s^2 along z (gravity), and turning it shows the rate about
// each axis. No orientation: this IMU doesn't work one out. Set its mount in Faber to say how the
// board sits on your machine.
//
// If it doesn't work: a GY-521 with nothing in its viewer isn't answering on I2C: check SDA and SCL
// (they are easy to swap) and its address. A DS3231 clock is at 0x68 too: then use 0x69.

#include <FaberLink.h>
#include <FaberImu.h>

FaberLink link(Serial);

FaberImu imu("imu");  // at 0x68, 20 readings a second

void setup() {
  link.begin(115200, "imu", "0.1.0");
}

void loop() {
  link.poll();
}
