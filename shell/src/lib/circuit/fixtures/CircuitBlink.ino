// Blinks D13, fades D9, and reads A0 and D2 (a button to GND, INPUT_PULLUP).
void setup() {
  Serial.begin(115200);
  pinMode(13, OUTPUT);
  pinMode(2, INPUT_PULLUP);
  Serial.println("Hello from Circuits");
}

void loop() {
  static int fade = 0;
  digitalWrite(13, (millis() / 500) % 2);
  analogWrite(9, fade);
  fade = (fade + 5) % 256;
  Serial.print("A0=");
  Serial.print(analogRead(A0));
  Serial.print(" button=");
  Serial.println(digitalRead(2) == LOW ? "pressed" : "up");
  delay(250);
}
