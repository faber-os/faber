// BatteryMonitor: a battery's voltage and level in Faber, measured through a voltage divider.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), two resistors (here 20 kOhm and 10 kOhm),
// the battery you want to watch (here a 3S Li-ion pack, 9.0-12.6 V), and a USB cable to the
// computer running Faber.
//
// Wiring (the divider brings 12.6 V down to 4.2 V, under the Uno's 5 V reference):
//   Battery +  -> 20 kOhm -> A1 -> 10 kOhm -> GND
//   Battery -  -> GND (the Arduino's and the battery's ground must be joined)
//
// What you should see: in Add part, "Check for a FaberLink board" on the board's port finds
// "battery-monitor". Add it (say as "arduino"): the battery appears as the part "arduino.battery"
// with its own viewer: the voltage twice a second, and its level, ok, low (below 10.5 V) or
// critical (below 9.9 V). Faber shows a notice whenever the level changes.
//
// If it doesn't work: if the voltage is a little off, measure the two resistors (or the voltage
// at A1 and at the battery) with a multimeter and put the real ratio below. If it reads about 0 V,
// check that the grounds are joined. Close the Arduino IDE's Serial Monitor: the port belongs to
// FaberLink.
//
// Never connect a battery straight to an analog pin: anything above the board's reference
// voltage (5 V on an Uno) can damage it. Check the divider's output with a multimeter first.

#include <FaberLink.h>

FaberLink link(Serial);

// A1, a divider ratio of (20 + 10) / 10 = 3.0, three Li-ion cells in series. The last two numbers
// are the analog reference (5 V on an Uno or Mega) and the pack's capacity in Ah (0 if unknown).
FaberBattery battery("battery", A1, 3.0, 3, faberlink::BATTERY_LI_ION, 5.0, 15.0);

void setup() {
  link.begin(115200, "battery-monitor", "0.1.0");
}

void loop() {
  link.poll();  // the library reads the battery twice a second; keep loop() free of delay()
}
