//! My app: watches one or more distance sensors and publishes the nearest distance any of them
//! measures, as `app:my-app/nearest`.
//!
//! An app never knows which parts it uses: it asks for roles in faber.toml (here `sensors`, any
//! parts that provide `distance@1`), and the user picks the parts. Everything it can do goes
//! through `host`: read its roles, drive the roles it may command, publish, keep a small state,
//! and log. The tests below run it on the PC with a pretend host, without wasm or a Faber machine.

use faber_app_wasm::{App, Host, LogLevel, Reading};
use faber_capabilities::distance;

#[derive(Default)]
pub struct MyApp {
    /// How many parts fill the `sensors` role.
    sensors: u32,
}

impl App for MyApp {
    fn start(&mut self, host: &mut dyn Host, _now_us: u64) {
        self.sensors = host.roles().iter().find(|r| r.name == "sensors").map_or(0, |r| r.parts);
        host.log(LogLevel::Info, &format!("watching {} distance sensors", self.sensors));
    }

    fn tick(&mut self, host: &mut dyn Host, now_us: u64) {
        // The newest reading of each sensor, keeping only real measurements.
        let nearest = (0..self.sensors)
            .filter_map(|i| match host.latest("sensors", i) {
                Some(Reading::Distance(d)) if d.status == distance::Status::Ok => Some(d.meters),
                _ => None,
            })
            .reduce(f32::min);
        let reading = match nearest {
            Some(meters) => distance::Reading { t_us: now_us, meters, status: distance::Status::Ok },
            None => distance::Reading { t_us: now_us, meters: 0.0, status: distance::Status::NoTarget },
        };
        host.publish("nearest", Reading::Distance(reading));
    }
}

#[cfg(target_arch = "wasm32")]
faber_app_wasm::export!(MyApp);

#[cfg(test)]
mod tests {
    use super::*;
    use faber_app_wasm::Role;
    use faber_app_wasm::testing::TestHost;

    fn d(meters: f32, status: distance::Status) -> Reading {
        Reading::Distance(distance::Reading { t_us: 1, meters, status })
    }

    #[test]
    fn it_publishes_the_nearest_real_measurement() {
        let sensors =
            Role { name: "sensors".into(), capability: "distance@1".into(), command: false, parts: 3 };
        let mut host = TestHost::with_roles(vec![sensors]);
        host.set_reading("sensors", 0, d(1.2, distance::Status::Ok));
        host.set_reading("sensors", 1, d(0.4, distance::Status::Ok));
        host.set_reading("sensors", 2, d(0.1, distance::Status::TooClose));
        let mut app = MyApp::default();
        app.start(&mut host, 0);
        app.tick(&mut host, 5);
        assert_eq!(
            host.published,
            [(
                "nearest".to_string(),
                Reading::Distance(distance::Reading { t_us: 5, meters: 0.4, status: distance::Status::Ok })
            )]
        );
    }
}
