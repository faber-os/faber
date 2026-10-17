/**
 * SENNEP V2's whole machine, as its electronics plan has it (SENNEP V2 repo:
 * docs/electronics/BREADBOARD_PLAN.md v0.2, §2.2 wire gauges, §2.4 e-stop, §2.6 level shifting,
 * §2.7 pin map = firmware/sennep_mega/config.h, §3 block diagram), built from the parts in
 * components/inventory.csv.
 *
 * Where the plan leaves something open, this takes the plan's own proposal and says so here:
 * - the LED strip's length (2 m, 60 LEDs) and its own 5 A buck: not in the plan;
 * - the Nano LED node on Serial3 at D0/D1 (its only UART);
 * - the e-stop's second contact to D23 is left out: the e-stop's contacts aren't measured yet (§8.4);
 * - the arm button on D29 (role still open, §2.4);
 * - the shifter's channel 3 (Pi GPIO4 → Mega RESET) is drawn up to the shifter; the lab can't wire RESET yet;
 * - R_IS and L_IS: only R_IS goes to A0–A3 here (the plan joins both through a resistor not chosen yet).
 */
import type { Diagram, Gauge, Part, Wire } from './types';

export function sennepV2(): Diagram {
  const parts: Part[] = [];
  const add = (id: string, def: string, name = id) => parts.push({ id, def, name, x: 0, y: 0, rot: 0 });
  const wires: Wire[] = [];
  let seq = 1;
  const w = (a: string, b: string, gauge?: Gauge, len?: number) => wires.push({ id: `w${seq++}`, a, b, ...(gauge ? { gauge } : {}), ...(len ? { len } : {}) });

  // ---- power: battery, BMS, XT60, main fuse, relay (motor bus) --------------------------------
  add('bat', 'bat?s=3&p=6&cell=4.2&nom=3.6&mah=3400', 'battery');
  add('bms', 'bms3s', 'bms');
  add('xt', 'xt60', 'xt60');
  add('fmain', 'blade?a=30', 'main-fuse');
  add('relay', 'relay40', 'motor-relay');
  add('dfly', '1n4007', 'D1');
  add('estop', 'estop', 'e-stop');
  add('tvs', 'p6ke15a', 'tvs');
  add('cbus', 'cap?uf=2200&v=25&polar=1', 'C-bus');
  w('bat.+', 'bms.B+', '10', 0.15);
  w('bat.-', 'bms.B-', '10', 0.15);
  w('bms.P+', 'xt.+A', '10', 0.1);
  w('bms.P-', 'xt.-A', '10', 0.1);
  w('xt.+B', 'fmain.A', '10', 0.08);
  w('fmain.B', 'relay.30', '10', 0.3);
  // The motor bus after the relay: TVS and the big capacitor across it.
  w('relay.87', 'tvs.K', '16', 0.1);
  w('tvs.A', 'xt.-B', '16', 0.1);
  w('relay.87', 'cbus.+', '16', 0.1);
  w('cbus.-', 'xt.-B', '16', 0.1);

  // ---- four corners: 15 A fuse, BTS7960, motor ------------------------------------------------
  const corners = [
    { c: 'fl', name: 'FL', rpwm: 'D2', lpwm: 'D3', is: 'A0' },
    { c: 'fr', name: 'FR', rpwm: 'D5', lpwm: 'D6', is: 'A1' },
    { c: 'rl', name: 'RL', rpwm: 'D7', lpwm: 'D8', is: 'A2' },
    { c: 'rr', name: 'RR', rpwm: 'D44', lpwm: 'D45', is: 'A3' },
  ];
  for (const k of corners) {
    add(`f${k.c}`, 'blade?a=15', `fuse-${k.c}`);
    add(`d${k.c}`, 'bts7960', `driver-${k.c}`);
    add(`m${k.c}`, 'dcmotor', `motor-${k.c}`);
    w('relay.87', `f${k.c}.A`, '16', 0.3);
    w(`f${k.c}.B`, `d${k.c}.B+`, '16', 0.2);
    w(`d${k.c}.B-`, 'xt.-B', '16', 0.4);
    w(`d${k.c}.M+`, `m${k.c}.M+`, '16', 0.3);
    w(`d${k.c}.M-`, `m${k.c}.M-`, '16', 0.3);
    w(`d${k.c}.RPWM`, `mega.${k.rpwm}`);
    w(`d${k.c}.LPWM`, `mega.${k.lpwm}`);
    w(`d${k.c}.R_EN`, 'mega.D22');
    w(`d${k.c}.L_EN`, 'mega.D22');
    w(`d${k.c}.R_IS`, `mega.${k.is}`);
    w(`d${k.c}.VCC`, 'mega.5V2');
    w(`d${k.c}.GND`, 'mega.GND4');
  }
  // All EN pins share D22, held low by 10 kΩ while the Mega boots.
  add('ren', 'res?ohms=10000&w=0.25', 'R-en');
  w('ren.1', 'mega.D22');
  w('ren.2', 'mega.GND4');

  // ---- logic branch: main switch, 5 A fuse, INA219, the two bucks -----------------------------
  add('sw', 'toggle', 'main-switch');
  add('flog', 'fuse?a=5', 'logic-fuse');
  add('ina', 'ina219', 'ina219');
  add('bpi', 'buckusb', 'pi-power');
  add('blog', 'buck5?v=5&a=3', 'logic-5v');
  w('fmain.B', 'sw.C', '16', 0.3);
  w('sw.1', 'flog.A', '16', 0.1);
  w('flog.B', 'ina.VIN+', '16', 0.1);
  w('ina.VIN-', 'bpi.IN+', '16', 0.2);
  w('ina.VIN-', 'blog.IN+', '16', 0.2);
  w('bpi.IN-', 'xt.-B', '16', 0.3);
  w('blog.IN-', 'xt.-B', '16', 0.3);
  // The e-stop breaks the relay coil, fed from the logic branch; the diode eats the coil's spike.
  w('ina.VIN-', 'estop.1', '22', 0.4);
  w('estop.2', 'relay.86', '22', 0.4);
  w('relay.85', 'xt.-B', '22', 0.3);
  w('dfly.K', 'relay.86');
  w('dfly.A', 'relay.85');

  // ---- compute: Pi 5 on its buck, Mega on the logic 5 V ---------------------------------------
  add('pi', 'pi5', 'pi');
  add('mega', 'mega', 'mega');
  w('bpi.USBC', 'pi.USBC');
  w('blog.OUT+', 'mega.5V1', '22', 0.2);
  w('blog.OUT-', 'mega.GND2', '22', 0.2);

  // The motor bus voltage on A4 (tells the Mega whether the e-stop is pressed): 47 kΩ / 10 kΩ.
  add('rtop', 'res?ohms=47000&w=0.25', 'R-bus-top');
  add('rbot', 'res?ohms=10000&w=0.25', 'R-bus-bottom');
  w('relay.87', 'rtop.1', '22', 0.4);
  w('rtop.2', 'mega.A4');
  w('rbot.1', 'mega.A4');
  w('rbot.2', 'mega.GND3');

  // ---- the Pi ↔ Mega link: Serial1 through the BSS138 shifter on a breadboard ----------------
  add('bb', 'breadboard', 'breadboard');
  add('lsh', 'shifter', 'shifter');
  w('bb.a4', 'pi.P10'); // LV1 → Pi RX (GPIO15) … HV1 ← Mega TX1 (18)
  w('bb.h4', 'mega.D18');
  w('bb.a5', 'pi.P8'); // LV2 ← Pi TX (GPIO14) … HV2 → Mega RX1 (19)
  w('bb.h5', 'mega.D19');
  w('bb.a6', 'pi.P1'); // LV = 3.3 V
  w('bb.h6', 'mega.5V3'); // HV = 5 V
  w('bb.a7', 'pi.P6');
  w('bb.h7', 'mega.GND5');
  w('bb.a8', 'pi.P7'); // LV3 ← GPIO4, for Mega RESET (flash from the Pi)

  // ---- on the Pi: LiDAR through its CP2102, GPS on uart3, camera, screen ----------------------
  add('lidar', 'ld19', 'lidar');
  add('cp', 'cp2102', 'lidar-usb');
  add('gps', 'neo6m', 'gps');
  add('cam', 'aicam', 'camera');
  add('screen', 'screen7', 'screen');
  w('lidar.P5V', 'blog.OUT+', '22', 0.5);
  w('lidar.GND', 'blog.OUT-', '22', 0.5);
  w('lidar.TX', 'cp.RXD');
  w('cp.GND', 'lidar.GND');
  w('cp.USB', 'pi.USB1');
  w('gps.VCC', 'blog.OUT+', '22', 0.5);
  w('gps.GND', 'blog.OUT-', '22', 0.5);
  w('gps.RX', 'pi.P24'); // GPIO8, uart3 TX
  w('gps.TX', 'pi.P21'); // GPIO9, uart3 RX
  w('pi.P25', 'blog.OUT-', '22', 0.3); // a ground between the Pi and the logic 5 V
  w('cam.CSI', 'pi.CAM0');
  w('screen.HDMI', 'pi.HDMI0');
  w('screen.USB', 'pi.USB2');

  // ---- on the Mega: I²C (multiplexer + 8 ToF, IMU, compass, INA219) ---------------------------
  add('tca', 'tca9548a', 'i2c-mux');
  add('imu', 'mpu6050', 'imu');
  add('mag', 'qmc5883l', 'compass');
  for (const id of ['tca', 'imu', 'mag', 'ina']) {
    const pwr = id === 'tca' ? 'VIN' : 'VCC';
    w(`${id}.${pwr}`, 'mega.5V3');
    w(`${id}.GND`, 'mega.GND5');
    w(`${id}.SDA`, 'mega.D20');
    w(`${id}.SCL`, 'mega.D21');
  }
  for (let i = 0; i < 8; i++) {
    add(`tof${i}`, 'vl53l0x', `tof-${i + 1}`);
    w(`tof${i}.VIN`, 'mega.5V3');
    w(`tof${i}.GND`, 'mega.GND5');
    w(`tof${i}.SDA`, `tca.SD${i}`);
    w(`tof${i}.SCL`, `tca.SC${i}`);
  }

  // ---- edge sensors (24–27), sonars (28 + A8–A11), arm button (29) ----------------------------
  ['fl', 'fr', 'rl', 'rr'].forEach((c, i) => {
    add(`edge${c}`, 'e18', `edge-${c}`);
    w(`edge${c}.VCC`, 'mega.5V2');
    w(`edge${c}.GND`, 'mega.GND4');
    w(`edge${c}.OUT`, `mega.D${24 + i}`);
    add(`sonar${c}`, 'hcsr04', `sonar-${c}`);
    w(`sonar${c}.VCC`, 'mega.5V2');
    w(`sonar${c}.GND`, 'mega.GND1');
    w(`sonar${c}.TRIG`, 'mega.D28');
    w(`sonar${c}.ECHO`, `mega.A${8 + i}`);
  });
  add('arm', 'button', 'arm');
  w('arm.1', 'mega.D29');
  w('arm.2', 'mega.GND1');

  // ---- RF joystick: NRF24 on SPI, its own 3.3 V regulator and 10 µF ---------------------------
  add('ld33', 'ld1117', '3v3');
  add('nrf', 'nrf24', 'radio');
  add('cnrf', 'cap?uf=10&v=25&polar=1', 'C-radio');
  w('ld33.IN', 'blog.OUT+', '22', 0.3);
  w('ld33.GND', 'blog.OUT-', '22', 0.3);
  w('ld33.OUT', 'nrf.VCC');
  w('nrf.GND', 'mega.GND1');
  w('cnrf.+', 'nrf.VCC');
  w('cnrf.-', 'nrf.GND');
  w('nrf.CE', 'mega.D48');
  w('nrf.CSN', 'mega.D53');
  w('nrf.SCK', 'mega.D52');
  w('nrf.MOSI', 'mega.D51');
  w('nrf.MISO', 'mega.D50');

  // ---- the LED node: a Nano on Serial3 driving the strip, on its own 5 A buck -----------------
  add('bled', 'buck5?v=5&a=5', 'led-5v');
  add('nano', 'nano', 'led-node');
  add('strip', 'ws2812?n=60', 'led-strip');
  add('rdin', 'res?ohms=330&w=0.25', 'R-din');
  add('cled', 'cap?uf=1000&v=25&polar=1', 'C-strip');
  w('ina.VIN-', 'bled.IN+', '16', 0.3);
  w('bled.IN-', 'xt.-B', '16', 0.3);
  w('bled.OUT+', 'strip.5V', '16', 0.3);
  w('bled.OUT-', 'strip.GND', '16', 0.3);
  w('cled.+', 'strip.5V');
  w('cled.-', 'strip.GND');
  w('bled.OUT+', 'nano.5V', '22', 0.2);
  w('bled.OUT-', 'nano.GND', '22', 0.2);
  w('nano.D6', 'rdin.1');
  w('rdin.2', 'strip.DIN');
  w('nano.D1', 'mega.D15'); // Nano TX → Mega RX3
  w('nano.D0', 'mega.D14'); // Mega TX3 → Nano RX
  w('nano.GND2', 'mega.GND3');

  parts.forEach((p, i) => {
    p.x = (i % 8) * 140;
    p.y = Math.floor(i / 8) * 110;
  });
  return {
    name: 'SENNEP V2',
    parts,
    wires,
    done: {},
    seq,
    // The firmware sets these to INPUT_PULLUP (config.h): e-stop sense, edge sensors, arm button.
    pullups: ['mega.D23', 'mega.D24', 'mega.D25', 'mega.D26', 'mega.D27', 'mega.D29'],
  };
}

/** Seats the shifter across the breadboard's gap (as in example.ts). */
export function seatSennepShifter(doc: Diagram): void {
  const bb = doc.parts.find((p) => p.id === 'bb');
  const sh = doc.parts.find((p) => p.id === 'lsh');
  if (!bb || !sh) return;
  sh.rot = 0;
  sh.x = bb.x + 4 + 2 * 3 - 2;
  sh.y = bb.y + 18;
}
