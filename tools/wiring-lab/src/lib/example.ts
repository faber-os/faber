/**
 * SENNEP V2's electronics around one corner, with the pins from its firmware
 * (sennep_mega/config.h): the 3S6P pack through its BMS, fuse and e-stop to the front-left driver
 * (D2/D3, shared enable D22); the Pi on its own USB-C buck; the Mega on the Pi's USB and on
 * Serial1 through a BSS138 shifter on a breadboard; the sonars on shared trigger D28 and echo
 * A8/A9; the LiDAR on the Pi through its CP2102 adapter.
 */
import type { Diagram, Part, Wire } from './types';

export function sennepExample(): Diagram {
  const parts: Part[] = [
    { id: 'pi', def: 'pi5', name: 'pi', x: 0, y: 0, rot: 0 },
    { id: 'mega', def: 'mega', name: 'mega', x: 0, y: 0, rot: 0 },
    { id: 'bb', def: 'breadboard', name: 'breadboard', x: 0, y: 0, rot: 0 },
    { id: 'lsh', def: 'shifter', name: 'shifter', x: 0, y: 0, rot: 0 },
    { id: 'sl', def: 'hcsr04', name: 'sonar-fl', x: 0, y: 0, rot: 0 },
    { id: 'sr', def: 'hcsr04', name: 'sonar-fr', x: 0, y: 0, rot: 0 },
    { id: 'imu', def: 'mpu6050', name: 'imu', x: 0, y: 0, rot: 0 },
    { id: 'drv', def: 'bts7960', name: 'drive-fl', x: 0, y: 0, rot: 0 },
    { id: 'mot', def: 'dcmotor', name: 'wheel-fl', x: 0, y: 0, rot: 0 },
    { id: 'bat', def: 'bat?s=3&p=6&cell=4.2&nom=3.6&mah=3400', name: 'battery', x: 0, y: 0, rot: 0 },
    { id: 'bms', def: 'bms3s', name: 'bms', x: 0, y: 0, rot: 0 },
    { id: 'fuse', def: 'fuse', name: 'main-fuse', x: 0, y: 0, rot: 0 },
    { id: 'estop', def: 'estop', name: 'e-stop', x: 0, y: 0, rot: 0 },
    { id: 'buck', def: 'buckusb', name: 'pi-power', x: 0, y: 0, rot: 0 },
    { id: 'arm', def: 'button', name: 'arm', x: 0, y: 0, rot: 0 },
    { id: 'lidar', def: 'ld19', name: 'lidar', x: 0, y: 0, rot: 0 },
    { id: 'usb', def: 'cp2102', name: 'lidar-usb', x: 0, y: 0, rot: 0 },
  ];
  const pairs: [string, string][] = [
    ['mega.USB', 'pi.USB3'],
    ['sl.TRIG', 'mega.D28'], ['sl.ECHO', 'mega.A8'], ['sl.VCC', 'mega.5V2'], ['sl.GND', 'mega.GND1'],
    ['sr.TRIG', 'mega.D28'], ['sr.ECHO', 'mega.A9'], ['sr.VCC', 'mega.5V2'], ['sr.GND', 'mega.GND1'],
    ['imu.SDA', 'mega.D20'], ['imu.SCL', 'mega.D21'], ['imu.VCC', 'mega.5V3'], ['imu.GND', 'mega.GND4'],
    ['drv.RPWM', 'mega.D2'], ['drv.LPWM', 'mega.D3'], ['drv.R_EN', 'mega.D22'], ['drv.L_EN', 'mega.D22'],
    ['drv.VCC', 'mega.5V1'], ['drv.GND', 'mega.GND2'],
    ['drv.M+', 'mot.M+'], ['drv.M-', 'mot.M-'],
    // Battery → BMS → fuse → e-stop → the motor driver; the Pi's buck before the e-stop.
    ['bat.+', 'bms.B+'], ['bat.-', 'bms.B-'],
    ['bms.P+', 'fuse.A'], ['fuse.B', 'estop.1'], ['estop.2', 'drv.B+'], ['bms.P-', 'drv.B-'],
    ['fuse.B', 'buck.IN+'], ['bms.P-', 'buck.IN-'], ['bms.P-', 'mega.GND3'],
    ['buck.USBC', 'pi.USBC'],
    // The LiDAR on the Pi through its USB-UART adapter.
    ['lidar.P5V', 'usb.5V'], ['lidar.GND', 'usb.GND'], ['lidar.TX', 'usb.RXD'], ['usb.USB', 'pi.USB1'],
    ['arm.1', 'mega.D29'], ['arm.2', 'mega.GND5'],
    // The serial link through the shifter, which sits across the breadboard's gap in columns 4–9
    // (LV row in row d, HV row in row f): TX1 18 → HV1 … LV1 → GPIO15; GPIO14 → LV2 … HV2 → RX1 19.
    ['bb.a4', 'pi.P10'], ['bb.h4', 'mega.D18'],
    ['bb.a5', 'pi.P8'], ['bb.h5', 'mega.D19'],
    ['bb.a6', 'pi.P1'], ['bb.h6', 'mega.5V3'],
    ['bb.a7', 'pi.P6'], ['bb.h7', 'mega.GND5'],
  ];
  // Apart to start with, so nothing plugs into the breadboard by accident; Arrange places them.
  parts.forEach((p, i) => {
    p.x = (i % 4) * 160;
    p.y = Math.floor(i / 4) * 120;
  });
  const wires: Wire[] = pairs.map(([a, b], i) => ({ id: `w${i + 1}`, a, b }));
  return { name: 'SENNEP · front-left corner', parts, wires, done: {}, seq: wires.length + 1 };
}

/** Put the shifter on the breadboard across the gap (LV row in row c, HV row in row f, columns 4–9). */
export function seatShifter(doc: Diagram): void {
  const bb = doc.parts.find((p) => p.id === 'bb');
  const sh = doc.parts.find((p) => p.id === 'lsh');
  if (!bb || !sh) return;
  sh.rot = 0;
  sh.x = bb.x + 4 + 2 * 3 - 2;
  sh.y = bb.y + 18;
}
