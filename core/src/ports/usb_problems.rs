//! "Plugged in, but the computer can't talk to it": things on USB that the OS sees but can't use.
//!
//! A serial adapter with a bad cable never becomes a port, so the port list alone would show
//! nothing at all. The OS does notice the attempt, though: Windows lists an "Unknown USB device
//! (Port reset failed)" with a problem code. This module finds those ([`UsbProblemDiscovery`]) and
//! turns them into plain-language [`PortProblem`]s ([`port_problem`]), so the user learns what
//! happened and what to try (docs/principles.md, "Errors say what to do").

/// Something on USB the OS reports as not working, as the OS describes it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FailedUsb {
    /// The OS's own id for it, e.g. `USB\VID_0000&PID_0001\5&DAB2676&0&5` on Windows. Only used
    /// to keep the list in a stable order.
    pub os_id: String,
    /// The OS's hardware ids, e.g. `USB\RESET_FAILURE`. Language-independent, unlike the name
    /// Windows shows (which is translated: "Ukendt USB-enhed (portnulstilling mislykkedes)").
    pub hardware_ids: Vec<String>,
    /// The Windows problem code (`ConfigManagerErrorCode`), e.g. 43. `None` where the OS has none.
    pub problem_code: Option<u32>,
}

/// What Faber tells the user about a [`FailedUsb`]: what happened, and what to try.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortProblem {
    /// Stable, kebab-case, e.g. `usb-no-answer`.
    pub code: &'static str,
    /// One line, e.g. "Plugged in, but the computer can't talk to it".
    pub label: String,
    /// What happened, in plain words.
    pub detail: String,
    /// The next thing to try.
    pub hint: String,
}

/// Finds USB things the OS sees but can't use. A trait so tests can fake them.
pub trait UsbProblemDiscovery: Send + Sync {
    fn list(&self) -> Vec<FailedUsb>;
}

/// The hint for every "can't talk to it" case: the usual causes, cheapest check first.
const CABLE_HINT: &str = "Unplug it and plug it back in firmly. If that doesn't help: try another USB cable (some \
cables only carry power, not data), plug it straight into the computer instead of through a long or unpowered \
extension or hub, and make sure it gets enough power (a motor or LED strip on the same supply can starve it).";

const CANT_TALK: &str = "Plugged in, but the computer can't talk to it";

/// Windows' problem code for "disabled by the user": not a fault, so never reported.
const DISABLED: u32 = 22;
/// "No driver installed".
const NO_DRIVER: u32 = 28;

/// Maps what the OS reports to what Faber tells the user. `None` when there is nothing to tell
/// (e.g. the user disabled it on purpose).
///
/// Windows marks a failed USB enumeration with a placeholder id `USB\VID_0000&PID_000x` and a
/// hardware id naming the step that failed. Those are matched by keyword, because only
/// `USB\RESET_FAILURE` has been seen on real hardware so far; the other names come from
/// Windows' "Unknown USB Device (…)" texts.
pub fn port_problem(f: &FailedUsb) -> Option<PortProblem> {
    if f.problem_code == Some(DISABLED) {
        return None;
    }
    let ids: Vec<String> = f.hardware_ids.iter().map(|h| h.to_ascii_uppercase()).collect();
    let has = |needle: &str| ids.iter().any(|h| h.contains(needle));
    let cant_talk = |code: &'static str, detail: &str, hint: &str| PortProblem {
        code,
        label: CANT_TALK.into(),
        detail: detail.into(),
        hint: hint.into(),
    };
    if has("RESET_FAILURE") {
        return Some(cant_talk(
            "usb-no-answer",
            "Something is plugged in over USB, but it didn't answer when the computer tried to start \
             talking to it (Windows: \"port reset failed\").",
            CABLE_HINT,
        ));
    }
    if has("DEVICE_DESCRIPTOR_FAILURE") || has("SET_ADDRESS_FAILURE") {
        return Some(cant_talk(
            "usb-garbled",
            "Something is plugged in over USB, but its answers didn't get through properly when the \
             computer asked what it is. That usually means the data wires in the cable or plug are \
             unreliable.",
            CABLE_HINT,
        ));
    }
    if has("VALIDATION_FAILURE") || (has("CONFIG") && has("FAILURE")) {
        return Some(cant_talk(
            "usb-invalid-answer",
            "Something is plugged in over USB and answers, but what it says doesn't follow the USB \
             rules. Either the connection is bad, or the program on the board has crashed or is broken.",
            "Unplug it and plug it back in. If it's a board you program yourself, press its reset button \
             or upload a known-good sketch. If that doesn't help, try another cable and plug it straight \
             into the computer.",
        ));
    }
    if has("COMPLIANCE_MODE") {
        return Some(cant_talk(
            "usb-link-failed",
            "Something is plugged into a fast (USB 3) socket, but the connection never came up.",
            "Unplug it and plug it back in. If it keeps happening, try a USB 2 socket or another cable.",
        ));
    }
    let placeholder = f.os_id.to_ascii_uppercase().contains("VID_0000&PID_");
    match f.problem_code {
        _ if placeholder => Some(cant_talk(
            "usb-not-recognised",
            "Something is plugged in over USB, but the computer couldn't find out what it is.",
            CABLE_HINT,
        )),
        Some(NO_DRIVER) => Some(PortProblem {
            code: "usb-no-driver",
            label: "Plugged in, but the computer has no driver for it".into(),
            detail: "Something is plugged in over USB and answers, but Windows has no driver for it, so \
                     it can't become a port."
                .into(),
            hint: "Install the driver from the maker of its USB chip. Cheap boards often use a CH340 or \
                   CP210x chip, and the chip's name is usually printed on it."
                .into(),
        }),
        Some(code) => Some(PortProblem {
            code: "usb-stopped",
            label: "Plugged in, but it isn't working".into(),
            detail: format!(
                "Something is plugged in over USB, but Windows stopped it because it reported a problem \
                 (code {code})."
            ),
            hint: CABLE_HINT.into(),
        }),
        None => None,
    }
}

/// The real OS discovery, picked per platform.
pub struct SystemUsbProblems;

impl UsbProblemDiscovery for SystemUsbProblems {
    fn list(&self) -> Vec<FailedUsb> {
        platform::list()
    }
}

/// Windows: one WMI query for present Plug and Play entries on the USB bus with a problem code.
///
/// Why WMI (through the `wmi` crate) rather than SetupAPI: the workspace forbids `unsafe`, and
/// SetupAPI can only be called with `unsafe` FFI. WMI gives the same data (present devices,
/// `ConfigManagerErrorCode`, hardware ids) through a safe, typed API, and needs no extra
/// process. The connection is made once per thread and reused, so each scan is a single local
/// query that only returns the (usually zero) entries with a problem.
#[cfg(windows)]
mod platform {
    use super::FailedUsb;
    use serde::Deserialize;
    use std::cell::RefCell;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Entry {
        #[serde(rename = "DeviceID")]
        device_id: String,
        #[serde(rename = "HardwareID")]
        hardware_id: Option<Vec<String>>,
        config_manager_error_code: Option<u32>,
    }

    // `Win32_PnPEntity` only lists what is present. `\\` is WQL for one backslash.
    const QUERY: &str = "SELECT DeviceID, HardwareID, ConfigManagerErrorCode FROM Win32_PnPEntity \
                         WHERE ConfigManagerErrorCode <> 0 AND DeviceID LIKE 'USB\\\\%'";

    thread_local! {
        // A WMI connection belongs to the thread that made it (COM); keep one per thread.
        static CONNECTION: RefCell<Option<wmi::WMIConnection>> = const { RefCell::new(None) };
    }

    pub fn list() -> Vec<FailedUsb> {
        match query() {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(target: "core::ports", "could not ask Windows for USB problems: {e}");
                // A broken connection is made again on the next scan.
                CONNECTION.with(|c| c.borrow_mut().take());
                Vec::new()
            }
        }
    }

    pub(super) fn query() -> wmi::WMIResult<Vec<FailedUsb>> {
        CONNECTION.with(|cell| {
            let mut cell = cell.borrow_mut();
            let conn = match cell.as_ref() {
                Some(c) => c,
                None => cell.insert(wmi::WMIConnection::new()?),
            };
            let rows: Vec<Entry> = conn.raw_query(QUERY)?;
            Ok(rows
                .into_iter()
                .map(|r| FailedUsb {
                    os_id: r.device_id,
                    hardware_ids: r.hardware_id.unwrap_or_default(),
                    problem_code: r.config_manager_error_code,
                })
                .collect())
        })
    }
}

/// Linux: nothing yet. A USB device that fails to enumerate never gets a sysfs entry or a udev
/// event, so the only trace is the kernel log ("device descriptor read/64, error -71"), and
/// reading that needs `CAP_SYSLOG` on Debian. See ai-instructions/FUTURE_IDEAS.md.
#[cfg(not(windows))]
mod platform {
    use super::FailedUsb;

    pub fn list() -> Vec<FailedUsb> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(os_id: &str, hardware_ids: &[&str], problem_code: Option<u32>) -> FailedUsb {
        FailedUsb {
            os_id: os_id.into(),
            hardware_ids: hardware_ids.iter().map(|s| s.to_string()).collect(),
            problem_code,
        }
    }

    #[test]
    fn port_reset_failed_says_the_computer_cant_talk_to_it() {
        // Exactly what Windows 11 reported for a cable on a long passive extension.
        let p =
            port_problem(&failed(r"USB\VID_0000&PID_0001\5&DAB2676&0&5", &[r"USB\RESET_FAILURE"], Some(43)))
                .unwrap();
        assert_eq!(p.code, "usb-no-answer");
        assert_eq!(p.label, "Plugged in, but the computer can't talk to it");
        assert!(p.detail.contains("didn't answer"), "{}", p.detail);
        assert!(p.hint.contains("another USB cable") && p.hint.contains("extension"), "{}", p.hint);
    }

    #[test]
    fn descriptor_and_address_failures_point_at_the_data_wires() {
        for id in [r"USB\DEVICE_DESCRIPTOR_FAILURE", r"USB\SET_ADDRESS_FAILURE"] {
            let p = port_problem(&failed(r"USB\VID_0000&PID_0002\1", &[id], Some(43))).unwrap();
            assert_eq!(p.code, "usb-garbled", "{id}");
            assert!(p.detail.contains("data wires"), "{id}");
            assert_eq!(p.label, CANT_TALK);
        }
    }

    #[test]
    fn invalid_descriptors_suggest_the_board_may_have_crashed() {
        let p = port_problem(&failed(
            r"USB\VID_0000&PID_0005\1",
            &[r"USB\DEVICE_DESCRIPTOR_VALIDATION_FAILURE"],
            Some(43),
        ))
        .unwrap();
        assert_eq!(p.code, "usb-invalid-answer");
        assert!(p.hint.contains("reset button"));
    }

    #[test]
    fn hardware_ids_are_matched_in_any_case() {
        let p = port_problem(&failed(r"usb\vid_0000&pid_0001\x", &[r"usb\reset_failure"], Some(43))).unwrap();
        assert_eq!(p.code, "usb-no-answer");
    }

    #[test]
    fn an_unknown_failure_id_is_still_reported() {
        let p = port_problem(&failed(r"USB\VID_0000&PID_0009\1", &[r"USB\SOMETHING_NEW"], Some(43))).unwrap();
        assert_eq!(p.code, "usb-not-recognised");
        assert_eq!(p.label, CANT_TALK);
    }

    #[test]
    fn a_missing_driver_says_which_driver_to_look_for() {
        let p = port_problem(&failed(r"USB\VID_1A86&PID_7523\5&1", &[r"USB\VID_1A86&PID_7523"], Some(28)))
            .unwrap();
        assert_eq!(p.code, "usb-no-driver");
        assert!(p.hint.contains("CH340"));
    }

    #[test]
    fn a_real_device_stopped_by_windows_gives_the_code() {
        let p = port_problem(&failed(r"USB\VID_10C4&PID_EA60\0001", &[r"USB\VID_10C4&PID_EA60"], Some(43)))
            .unwrap();
        assert_eq!(p.code, "usb-stopped");
        assert!(p.detail.contains("code 43"));
    }

    #[test]
    fn disabled_on_purpose_is_not_a_problem() {
        assert_eq!(port_problem(&failed(r"USB\VID_10C4&PID_EA60\0001", &[], Some(22))), None);
        assert_eq!(
            port_problem(&failed(r"USB\VID_0000&PID_0001\1", &[r"USB\RESET_FAILURE"], Some(22))),
            None
        );
    }

    #[test]
    fn no_problem_code_and_no_failure_id_is_not_a_problem() {
        assert_eq!(port_problem(&failed(r"USB\VID_10C4&PID_EA60\0001", &[], None)), None);
    }

    #[test]
    fn user_facing_text_avoids_the_word_device() {
        // "device" is ambiguous in Faber (docs/glossary.md); Windows' own terms are quoted only.
        let all = [
            failed(r"USB\VID_0000&PID_0001\1", &[r"USB\RESET_FAILURE"], Some(43)),
            failed(r"USB\VID_0000&PID_0002\1", &[r"USB\DEVICE_DESCRIPTOR_FAILURE"], Some(43)),
            failed(
                r"USB\VID_0000&PID_0005\1",
                &[r"USB\CONFIGURATION_DESCRIPTOR_VALIDATION_FAILURE"],
                Some(43),
            ),
            failed(r"USB\VID_0000&PID_0007\1", &[r"USB\COMPLIANCE_MODE"], Some(43)),
            failed(r"USB\VID_0000&PID_0009\1", &[], Some(43)),
            failed(r"USB\VID_1A86&PID_7523\1", &[], Some(28)),
            failed(r"USB\VID_1A86&PID_7523\1", &[], Some(10)),
        ];
        for f in &all {
            let p = port_problem(f).unwrap();
            for text in [&p.label, &p.detail, &p.hint] {
                assert!(!text.to_lowercase().contains("device"), "{}: {text}", p.code);
            }
        }
    }

    /// Asks Windows for real. The query must succeed and be cheap; what it finds depends on what
    /// is plugged in, so nothing specific is asserted.
    #[cfg(windows)]
    #[test]
    fn the_real_windows_query_works() {
        let start = std::time::Instant::now();
        let found = platform::query().expect("the WMI query works");
        let first = start.elapsed();
        let again = std::time::Instant::now();
        platform::query().expect("the WMI query works a second time, on the kept connection");
        let warm = again.elapsed();
        let problems: Vec<_> = found.iter().filter_map(port_problem).collect();
        eprintln!(
            "{} USB problem(s) here: {problems:?}; first scan {first:?}, warm scan {warm:?}",
            found.len()
        );
        assert!(warm < std::time::Duration::from_secs(1), "a warm scan took {warm:?}; it runs every 2 s");
    }
}
