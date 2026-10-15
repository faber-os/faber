//! NMEA 0183, the text lines GPS modules send: `$GPGGA,…*47`. Reads GGA (position and fix),
//! RMC (speed and course) and GSA (2D or 3D), and puts them together into `position@1`.
//!
//! Every sentence must carry its checksum; one that doesn't add up is a bad line. Other
//! sentences with a good checksum (GSV, VTG, TXT, …) are fine and simply not used.

use faber_capabilities::position::{Fix, Reading};
use faber_capabilities::{Timestamp, position};

/// Metres per second in one knot.
const KNOT_M_S: f64 = 0.514_444;

/// One sentence, as far as `position@1` needs it.
#[derive(Clone, Debug, PartialEq)]
pub enum Sentence {
    Gga(Gga),
    Rmc(Rmc),
    /// GSA: 2 = a 2D fix, 3 = a 3D fix, 1 = none.
    Gsa {
        mode: u8,
    },
    /// A sentence with a good checksum that `position@1` doesn't need.
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gga {
    pub time: String,
    /// `None` while there is no fix.
    pub lat_lon: Option<(f64, f64)>,
    /// 0 none, 1 GPS, 2 DGPS, 4 RTK fixed, 5 RTK float, 6 estimated, …
    pub quality: u8,
    pub satellites: u8,
    pub altitude_m: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rmc {
    pub time: String,
    /// `A`: valid. `V`: no fix.
    pub valid: bool,
    pub lat_lon: Option<(f64, f64)>,
    pub speed_m_s: Option<f32>,
    pub course_rad: Option<f32>,
}

/// Reads one line. `Err` says why it isn't a usable NMEA sentence.
pub fn parse(line: &str) -> Result<Sentence, String> {
    let Some(body) = line.strip_prefix('$') else {
        return Err("it doesn't start with $, so it isn't an NMEA sentence".into());
    };
    let Some((data, sum)) = body.rsplit_once('*') else {
        return Err("it has no checksum (*hh at the end)".into());
    };
    let Ok(expected) = u8::from_str_radix(sum, 16) else {
        return Err(format!("its checksum \"{sum}\" is not two hexadecimal digits"));
    };
    if sum.len() != 2 {
        return Err(format!("its checksum \"{sum}\" is not two hexadecimal digits"));
    }
    let actual = data.bytes().fold(0u8, |a, b| a ^ b);
    if actual != expected {
        return Err(format!(
            "its checksum is {sum} but its text adds up to {actual:02X}; it was damaged on the way"
        ));
    }
    let fields: Vec<&str> = data.split(',').collect();
    let kind = fields[0].get(2..).unwrap_or("");
    let f = |i: usize| fields.get(i).copied().unwrap_or("");
    match kind {
        "GGA" => Ok(Sentence::Gga(Gga {
            time: f(1).to_string(),
            lat_lon: lat_lon(f(2), f(3), f(4), f(5))?,
            quality: number::<u8>(f(6), "fix quality")?.unwrap_or(0),
            satellites: number::<u8>(f(7), "satellite count")?.unwrap_or(0),
            altitude_m: number::<f32>(f(9), "altitude")?,
        })),
        "RMC" => Ok(Sentence::Rmc(Rmc {
            time: f(1).to_string(),
            valid: f(2) == "A",
            lat_lon: lat_lon(f(3), f(4), f(5), f(6))?,
            speed_m_s: number::<f64>(f(7), "speed")?.map(|k| (k * KNOT_M_S) as f32),
            course_rad: number::<f32>(f(8), "course")?.map(f32::to_radians),
        })),
        "GSA" => Ok(Sentence::Gsa { mode: number::<u8>(f(2), "fix mode")?.unwrap_or(1) }),
        _ => Ok(Sentence::Other),
    }
}

/// An empty field is `None`; anything else must be a number.
fn number<T: std::str::FromStr>(s: &str, what: &str) -> Result<Option<T>, String> {
    if s.is_empty() {
        return Ok(None);
    }
    s.parse().map(Some).map_err(|_| format!("its {what} \"{s}\" is not a number"))
}

/// `ddmm.mmmm,N,dddmm.mmmm,E` → degrees. Empty while there is no fix.
fn lat_lon(lat: &str, ns: &str, lon: &str, ew: &str) -> Result<Option<(f64, f64)>, String> {
    if lat.is_empty() || lon.is_empty() {
        return Ok(None);
    }
    let lat = degrees(lat, 2, "latitude")?;
    let lon = degrees(lon, 3, "longitude")?;
    let lat = match ns {
        "N" => lat,
        "S" => -lat,
        _ => return Err(format!("its latitude hemisphere \"{ns}\" is not N or S")),
    };
    let lon = match ew {
        "E" => lon,
        "W" => -lon,
        _ => return Err(format!("its longitude hemisphere \"{ew}\" is not E or W")),
    };
    if lat.abs() > 90.0 || lon.abs() > 180.0 {
        return Err("its position is off the globe".into());
    }
    Ok(Some((lat, lon)))
}

fn degrees(s: &str, deg_digits: usize, what: &str) -> Result<f64, String> {
    let bad = || format!("its {what} \"{s}\" is not degrees and minutes");
    let (d, m) = (s.get(..deg_digits).ok_or_else(bad)?, s.get(deg_digits..).ok_or_else(bad)?);
    let (d, m): (u16, f64) = (d.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if !(0.0..60.0).contains(&m) {
        return Err(bad());
    }
    Ok(f64::from(d) + m / 60.0)
}

/// Puts GGA, RMC and GSA together. GPS modules send RMC before GGA for the same second, so a GGA
/// takes the speed and course of the RMC with the same time. A module that sends no GGA at all
/// (two RMCs in a row) gets a reading from each RMC instead.
#[derive(Default)]
pub struct Assembler {
    last_rmc: Option<Rmc>,
    /// From the latest GSA: whether the fix is 2D or 3D.
    mode: Option<u8>,
    /// RMCs since the last GGA.
    rmc_without_gga: u32,
}

impl Assembler {
    /// The reading this sentence completes, if any.
    pub fn push(&mut self, t_us: Timestamp, sentence: Sentence) -> Option<Reading> {
        match sentence {
            Sentence::Gsa { mode } => {
                self.mode = Some(mode);
                None
            }
            Sentence::Other => None,
            Sentence::Rmc(rmc) => {
                self.rmc_without_gga = self.rmc_without_gga.saturating_add(1);
                let reading = (self.rmc_without_gga >= 2).then(|| self.rmc_reading(t_us, &rmc));
                self.last_rmc = Some(rmc);
                reading
            }
            Sentence::Gga(gga) => {
                self.rmc_without_gga = 0;
                Some(self.gga_reading(t_us, &gga))
            }
        }
    }

    /// The fix a plain GPS fix is: what GSA says, else 3D (GGA reports an altitude).
    fn gps_fix(&self) -> Fix {
        match self.mode {
            Some(2) => Fix::Fix2d,
            _ => Fix::Fix3d,
        }
    }

    fn gga_reading(&self, t_us: Timestamp, gga: &Gga) -> Reading {
        let fix = match gga.quality {
            1 | 3 => self.gps_fix(),
            2 => Fix::Dgps,
            4 => Fix::RtkFixed,
            5 => Fix::RtkFloat,
            // 0 no fix, 6 dead reckoning, 7 manual input, 8 simulation: not a measured position.
            _ => Fix::None,
        };
        let Some((latitude_deg, longitude_deg)) = gga.lat_lon.filter(|_| fix != Fix::None) else {
            return Reading::no_fix(t_us, gga.satellites);
        };
        let rmc = self.last_rmc.as_ref().filter(|r| r.valid && r.time == gga.time);
        Reading {
            t_us,
            latitude_deg,
            longitude_deg,
            altitude_m: gga.altitude_m,
            fix,
            satellites: gga.satellites,
            horizontal_accuracy_m: None,
            vertical_accuracy_m: None,
            speed_m_s: rmc.and_then(|r| r.speed_m_s),
            course_rad: rmc.and_then(|r| r.course_rad),
        }
    }

    fn rmc_reading(&self, t_us: Timestamp, rmc: &Rmc) -> Reading {
        let Some((latitude_deg, longitude_deg)) = rmc.lat_lon.filter(|_| rmc.valid) else {
            return Reading::no_fix(t_us, 0);
        };
        // RMC says nothing about 2D or 3D: GSA does, and without it 2D is the honest guess.
        let fix = if self.mode == Some(3) { Fix::Fix3d } else { Fix::Fix2d };
        position::Reading {
            latitude_deg,
            longitude_deg,
            fix,
            speed_m_s: rmc.speed_m_s,
            course_rad: rmc.course_rad,
            ..Reading::no_fix(t_us, 0)
        }
    }
}

/// `$` + `body` + `*` + its checksum: for tests and for writing example captures.
pub fn sentence(body: &str) -> String {
    format!("${body}*{:02X}", body.bytes().fold(0u8, |a, b| a ^ b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The GGA example quoted in most NMEA guides, with the checksum they all print for it.
    const GGA: &str = "$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47";

    #[test]
    fn checksums_are_computed_like_the_standard() {
        assert_eq!(sentence("GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,"), GGA);
    }

    #[test]
    fn gga_gives_position_fix_satellites_and_altitude() {
        let Sentence::Gga(g) = parse(GGA).unwrap() else { panic!() };
        let (lat, lon) = g.lat_lon.unwrap();
        assert!((lat - (48.0 + 7.038 / 60.0)).abs() < 1e-9);
        assert!((lon - (11.0 + 31.0 / 60.0)).abs() < 1e-9);
        assert_eq!((g.quality, g.satellites, g.altitude_m), (1, 8, Some(545.4)));
    }

    #[test]
    fn south_and_west_are_negative() {
        let s = sentence("GNGGA,000000,3351.000,S,15112.000,W,1,05,1.0,10.0,M,,M,,");
        let Sentence::Gga(g) = parse(&s).unwrap() else { panic!() };
        let (lat, lon) = g.lat_lon.unwrap();
        assert!(lat < -33.8 && lon < -151.1, "{lat} {lon}");
    }

    #[test]
    fn rmc_gives_speed_and_course() {
        let s = sentence("GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W");
        let Sentence::Rmc(r) = parse(&s).unwrap() else { panic!() };
        assert!(r.valid);
        assert!((r.speed_m_s.unwrap() - 11.523_546).abs() < 1e-3, "22.4 knots");
        assert!((r.course_rad.unwrap() - 84.4f32.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn damaged_or_foreign_lines_say_why() {
        let damaged = GGA.replace("4807", "4808");
        let cases = [
            (damaged.as_str(), "damaged"),
            ("GPGGA,1*00", "doesn't start with $"),
            ("$GPGGA,123519", "no checksum"),
            ("$GPGGA*4", "two hexadecimal"),
            ("$GPGGA*ZZ", "two hexadecimal"),
        ];
        for (line, words) in cases {
            assert!(parse(line).unwrap_err().contains(words), "{line}: {:?}", parse(line));
        }
        let bad_lat = sentence("GPGGA,1,48x7.0,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,");
        assert!(parse(&bad_lat).unwrap_err().contains("latitude"));
        let bad_hemisphere = sentence("GPGGA,1,4807.0,Q,01131.000,E,1,08,0.9,545.4,M,46.9,M,,");
        assert!(parse(&bad_hemisphere).unwrap_err().contains("N or S"));
    }

    #[test]
    fn other_sentences_are_fine_but_unused() {
        assert_eq!(parse(&sentence("GPGSV,1,1,00")).unwrap(), Sentence::Other);
        assert_eq!(
            parse(&sentence("GPGSA,A,3,04,05,,,,,,,,,,,2.5,1.3,2.1")).unwrap(),
            Sentence::Gsa { mode: 3 }
        );
    }

    fn push(a: &mut Assembler, t: Timestamp, body: &str) -> Option<Reading> {
        a.push(t, parse(&sentence(body)).unwrap())
    }

    #[test]
    fn gga_publishes_with_the_speed_of_the_rmc_from_the_same_second() {
        let mut a = Assembler::default();
        assert_eq!(
            push(&mut a, 1, "GPRMC,120000,A,5540.566,N,01234.100,E,1.0,90.0,151026,,"),
            None,
            "the GGA of the same second follows"
        );
        assert_eq!(push(&mut a, 2, "GPGSA,A,3,,,,,,,,,,,,,2.0,1.0,1.7"), None);
        let r = push(&mut a, 3, "GPGGA,120000,5540.566,N,01234.100,E,1,07,1.0,12.5,M,41.0,M,,").unwrap();
        assert_eq!((r.t_us, r.fix, r.satellites, r.altitude_m), (3, Fix::Fix3d, 7, Some(12.5)));
        assert_eq!(r.speed_m_s, Some(KNOT_M_S as f32), "the same second's RMC");
        assert_eq!(
            push(&mut a, 4, "GPRMC,120001,A,5540.566,N,01234.100,E,2.0,90.0,151026,,"),
            None,
            "GGA leads now"
        );
        let r = push(&mut a, 5, "GPGGA,120002,5540.566,N,01234.100,E,2,07,1.0,12.5,M,41.0,M,,").unwrap();
        assert_eq!((r.fix, r.speed_m_s), (Fix::Dgps, None), "an older RMC's speed is not used");
    }

    #[test]
    fn a_module_without_gga_gets_readings_from_rmc() {
        let mut a = Assembler::default();
        let rmc = |t: &str| format!("GPRMC,{t},A,5540.566,N,01234.100,E,1.0,90.0,151026,,");
        assert_eq!(push(&mut a, 1, &rmc("120000")), None);
        assert_eq!(
            push(&mut a, 2, &rmc("120001")),
            Some(Reading {
                latitude_deg: 55.0 + 40.566 / 60.0,
                longitude_deg: 12.0 + 34.1 / 60.0,
                fix: Fix::Fix2d,
                speed_m_s: Some(KNOT_M_S as f32),
                course_rad: Some(90f32.to_radians()),
                ..Reading::no_fix(2, 0)
            }),
            "two RMCs without a GGA: RMC is all there is, and without GSA 2D is the honest guess"
        );
        push(&mut a, 3, "GPGSA,A,3,,,,,,,,,,,,,2.0,1.0,1.7");
        assert_eq!(push(&mut a, 4, &rmc("120002")).unwrap().fix, Fix::Fix3d);
    }

    #[test]
    fn no_fix_is_published_as_none() {
        let mut a = Assembler::default();
        let r = push(&mut a, 9, "GPGGA,120000,,,,,0,03,,,M,,M,,").unwrap();
        assert_eq!(r, Reading::no_fix(9, 3));
        let r = push(&mut a, 10, "GPGGA,120000,5540.566,N,01234.100,E,6,03,,,M,,M,,").unwrap();
        assert_eq!(r.fix, Fix::None, "dead reckoning is not a measured position");
        let mut a = Assembler::default();
        push(&mut a, 1, "GPRMC,120000,V,,,,,,,151026,,");
        assert_eq!(push(&mut a, 2, "GPRMC,120001,V,,,,,,,151026,,").unwrap().fix, Fix::None);
    }
}
