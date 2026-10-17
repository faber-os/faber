/**
 * SENNEP V2's front-left corner, with the pins from its firmware (sennep_mega/config.h): the Pi
 * talks to the Mega over Serial1 through a BSS138 shifter on a breadboard, the FL driver uses
 * D2/D3 and the shared enable D22, the sonars share trigger D28 and echo on A8/A9.
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
    { id: 'bat', def: 'bat3s', name: 'battery', x: 0, y: 0, rot: 0 },
    { id: 'buck', def: 'buck5', name: 'pi-power', x: 0, y: 0, rot: 0 },
    { id: 'arm', def: 'button', name: 'arm', x: 0, y: 0, rot: 0 },
  ];
  const pairs: [string, string][] = [
    ['mega.USB', 'pi.USB3'],
    ['sl.TRIG', 'mega.D28'], ['sl.ECHO', 'mega.A8'], ['sl.VCC', 'mega.5V2'], ['sl.GND', 'mega.GND1'],
    ['sr.TRIG', 'mega.D28'], ['sr.ECHO', 'mega.A9'], ['sr.VCC', 'mega.5V2'], ['sr.GND', 'mega.GND1'],
    ['imu.SDA', 'mega.D20'], ['imu.SCL', 'mega.D21'], ['imu.VCC', 'mega.5V3'], ['imu.GND', 'mega.GND4'],
    ['drv.RPWM', 'mega.D2'], ['drv.LPWM', 'mega.D3'], ['drv.R_EN', 'mega.D22'], ['drv.L_EN', 'mega.D22'],
    ['drv.VCC', 'mega.5V1'], ['drv.GND', 'mega.GND2'],
    ['drv.M+', 'mot.M+'], ['drv.M-', 'mot.M-'],
    ['bat.+', 'drv.B+'], ['bat.-', 'drv.B-'], ['bat.-', 'mega.GND3'],
    ['bat.+', 'buck.IN+'], ['bat.-', 'buck.IN-'],
    ['buck.OUT+', 'pi.PWR'], ['buck.OUT-', 'pi.PWR-'],
    ['arm.1', 'mega.D29'], ['arm.2', 'mega.GND5'],
    // The serial link through the shifter, which sits across the breadboard's gap in columns 4–9
    // (LV row in row c, HV row in row f): TX1 18 → HV1 … LV1 → GPIO15; GPIO14 → LV2 … HV2 → RX1 19.
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
  sh.y = bb.y + 16;
}
