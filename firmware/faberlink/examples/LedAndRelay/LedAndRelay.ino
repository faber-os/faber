// LedAndRelay: a dimmable LED and a relay module, switched from Faber.
//
// What you need: an Arduino (Uno, Nano, Mega, ...), an LED with a 220 Ohm resistor, a 5 V relay
// module (one channel, with its own transistor and diode, as almost all are), jumper wires, and a
// USB cable to the computer running Faber. Switch nothing above 30 V with the relay while you try
// this: a lamp at mains voltage is not a first project.
//
// Wiring:
//   LED: pin 5 -> 220 Ohm resistor -> LED's long leg; LED's short leg -> GND
//   Relay module: VCC -> 5V, GND -> GND, IN -> pin 7
//
// What you should see: add the board in Faber (say as "arduino"). "arduino.lamp" is a PWM output:
// set its duty and the LED dims. "arduino.relay" is an on/off output: switch it on and the relay
// clicks. Like a motor, each needs a lease to be commanded, and goes off when nobody has sent a
// command for a second, on the e-stop, and when Faber goes quiet.
//
// Most relay modules switch on when their IN is LOW (active low); this one is declared that way.
// If yours clicks on at the wrong times, it is active high: drop faberlink::ACTIVE_LOW.

#include <FaberLink.h>

FaberLink link(Serial);

FaberOutput lamp("lamp", 5, faberlink::OUTPUT_PWM);                             // pin 5 is a PWM pin (~)
FaberOutput relay("relay", 7, faberlink::OUTPUT_ON_OFF, faberlink::ACTIVE_LOW);  // on while pin 7 is LOW

void setup() {
  link.begin(115200, "outputs", "0.1.0");
}

void loop() {
  link.poll();
}
