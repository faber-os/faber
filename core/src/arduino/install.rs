//! "Install Arduino tools": `arduino-cli` from Arduino's official GitHub releases into
//! `<data>/tools/arduino-cli/`, checked against the SHA-256 Arduino publishes next to it.
//!
//! The download goes through [`Downloader`], so the tests serve files from memory and never
//! touch the network. The AVR core is installed afterwards by `arduino-cli` itself
//! ([`super::Toolchain::install_avr_core`]).

use super::toolchain::{exe_name, faber_dir};
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// The `arduino-cli` Faber installs. Moving to a newer one is a one-line change here, after
/// trying it (docs/arduino-editor.md).
pub const CLI_VERSION: &str = "1.5.1";

/// About how much "Install Arduino tools" downloads: `arduino-cli` (~30 MB unpacked, ~10 MB
/// packed) and the AVR core with its compiler (~140 MB), for the confirm dialog.
pub const APPROX_DOWNLOAD_MB: u32 = 150;

const RELEASES: &str = "https://github.com/arduino/arduino-cli/releases/download";

/// Fetches a URL into a file.
pub trait Downloader: Send + Sync {
    /// Writes the body of `url` to `to`. `progress` gets the bytes so far and, when the server
    /// says it, the total.
    fn download(
        &self,
        url: &str,
        to: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), String>;
}

/// One release file for this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: &'static str,
    /// `arduino-cli_1.5.1_Linux_ARM64.tar.gz`
    pub archive: String,
    pub archive_url: String,
    /// The release's list of SHA-256 sums: `1.5.1-checksums.txt`.
    pub checksums_url: String,
}

impl Release {
    /// The release for an OS and CPU as Rust names them (`std::env::consts`), or `None` where
    /// Arduino publishes none.
    pub fn for_platform(os: &str, arch: &str) -> Option<Release> {
        let (suffix, ext) = match (os, arch) {
            ("windows", "x86_64") => ("Windows_64bit", "zip"),
            ("windows", "x86") => ("Windows_32bit", "zip"),
            ("linux", "x86_64") => ("Linux_64bit", "tar.gz"),
            ("linux", "aarch64") => ("Linux_ARM64", "tar.gz"),
            ("linux", "arm") => ("Linux_ARMv7", "tar.gz"),
            ("macos", "x86_64") => ("macOS_64bit", "tar.gz"),
            ("macos", "aarch64") => ("macOS_ARM64", "tar.gz"),
            _ => return None,
        };
        let v = CLI_VERSION;
        let archive = format!("arduino-cli_{v}_{suffix}.{ext}");
        Some(Release {
            version: v,
            archive_url: format!("{RELEASES}/v{v}/{archive}"),
            checksums_url: format!("{RELEASES}/v{v}/{v}-checksums.txt"),
            archive,
        })
    }

    pub fn for_this_computer() -> Option<Release> {
        Release::for_platform(std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Where installing stands, for the progress bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Downloading { done: u64, total: Option<u64> },
    Verifying,
    Unpacking,
}

/// Why installing failed, with what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl InstallError {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> InstallError {
        InstallError { code, message: message.into(), hint: hint.into() }
    }
}

fn disk(e: io::Error) -> InstallError {
    InstallError::new(
        "io-error",
        format!("Faber could not write the Arduino tools to its data folder: {e}"),
        "Check that the disk isn't full, then try again.",
    )
}

/// The SHA-256 the checksums file gives `file` (lines `<hex>  <file name>`).
pub fn published_sha256(checksums: &str, file: &str) -> Option<String> {
    checksums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (sum, name) = (it.next()?, it.next()?);
        (name.trim_start_matches('*') == file
            && sum.len() == 64
            && sum.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| sum.to_ascii_lowercase())
    })
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The `arduino-cli` program inside a release archive.
fn extract(archive: &Path, name: &str) -> Result<Vec<u8>, InstallError> {
    let bad = |why: String| {
        InstallError::new(
            "download-damaged",
            format!("The downloaded Arduino tools can't be unpacked: {why}."),
            "Try again; if it keeps failing, the download is being changed on the way.",
        )
    };
    let exe = exe_name();
    let file = std::fs::File::open(archive).map_err(disk)?;
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(file).map_err(|e| bad(e.to_string()))?;
        let mut entry = zip.by_name(exe).map_err(|_| bad(format!("{exe} isn't in it")))?;
        let mut out = Vec::new();
        entry.read_to_end(&mut out).map_err(|e| bad(e.to_string()))?;
        return Ok(out);
    }
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(file).read_to_end(&mut tar).map_err(|e| bad(e.to_string()))?;
    tar_file(&tar, exe).ok_or_else(|| bad(format!("{exe} isn't in it")))
}

/// One file of a (ustar or GNU) tar archive, by its name at the top of the archive.
pub fn tar_file(tar: &[u8], wanted: &str) -> Option<Vec<u8>> {
    let mut at = 0;
    let mut long_name: Option<String> = None;
    while at + 512 <= tar.len() {
        let h = &tar[at..at + 512];
        if h.iter().all(|&b| b == 0) {
            return None;
        }
        let field = |r: std::ops::Range<usize>| {
            let f = &h[r];
            String::from_utf8_lossy(&f[..f.iter().position(|&b| b == 0).unwrap_or(f.len())]).into_owned()
        };
        let size = u64::from_str_radix(field(124..136).trim(), 8).ok()? as usize;
        let kind = h[156];
        let body = at + 512;
        let end = body.checked_add(size)?;
        if end > tar.len() {
            return None;
        }
        let prefix = field(345..500);
        let short = field(0..100);
        let name =
            long_name.take().unwrap_or(if prefix.is_empty() { short } else { format!("{prefix}/{short}") });
        match kind {
            b'L' => {
                long_name = Some(String::from_utf8_lossy(&tar[body..end]).trim_end_matches('\0').to_string())
            }
            b'0' | 0 if name.trim_start_matches("./") == wanted => return Some(tar[body..end].to_vec()),
            _ => {}
        }
        at = body + size.div_ceil(512) * 512;
    }
    None
}

/// The config Faber's own copy runs with: its data, downloads and libraries stay under
/// `<data>/tools/`, away from any Arduino IDE on the same computer.
pub fn config_text(tools: &Path) -> String {
    let p = |name: &str| tools.join(name).to_string_lossy().replace('\\', "/");
    format!(
        "# Faber's own arduino-cli (docs/arduino-editor.md). Faber writes this file.\n\
         directories:\n  data: \"{}\"\n  downloads: \"{}\"\n  user: \"{}\"\n",
        p("arduino15"),
        p("arduino15/staging"),
        p("sketchbook"),
    )
}

/// Downloads, checks and unpacks `arduino-cli` into `<data>/tools/arduino-cli/`, and writes its
/// config. Nothing is left in place unless the SHA-256 matched. Returns the program's path.
pub fn install_cli(
    downloader: &dyn Downloader,
    data_dir: &Path,
    release: &Release,
    step: &mut dyn FnMut(Step),
) -> Result<PathBuf, InstallError> {
    let dir = faber_dir(data_dir);
    let staging = data_dir.join("tools").join(".download");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(disk)?;
    let result = (|| {
        let unreachable = |what: &str, e: String| {
            InstallError::new(
                "download-failed",
                format!("Faber could not download {what}: {e}."),
                "Check that this machine is on the internet (its network cable or Wi-Fi), then try again.",
            )
        };
        let sums_file = staging.join("checksums.txt");
        downloader
            .download(&release.checksums_url, &sums_file, &mut |_, _| {})
            .map_err(|e| unreachable("the list of checksums", e))?;
        let sums = std::fs::read_to_string(&sums_file).map_err(disk)?;
        let expected = published_sha256(&sums, &release.archive).ok_or_else(|| {
            InstallError::new(
                "checksum-missing",
                format!("Arduino's list of checksums has no line for {}.", release.archive),
                "Faber won't install a file it can't check. Update Faber, or install arduino-cli yourself.",
            )
        })?;
        let archive = staging.join(&release.archive);
        downloader
            .download(&release.archive_url, &archive, &mut |done, total| {
                step(Step::Downloading { done, total })
            })
            .map_err(|e| unreachable("arduino-cli", e))?;
        step(Step::Verifying);
        let got = sha256_file(&archive).map_err(disk)?;
        if got != expected {
            return Err(InstallError::new(
                "checksum-mismatch",
                format!(
                    "The downloaded {} isn't the file Arduino published: its SHA-256 is {got}, not {expected}.",
                    release.archive
                ),
                "Nothing was installed. Try again; if it keeps happening, the download is being changed on the way.",
            ));
        }
        step(Step::Unpacking);
        let program = extract(&archive, &release.archive)?;
        std::fs::create_dir_all(&dir).map_err(disk)?;
        let tmp = dir.join(format!("{}.new", exe_name()));
        std::fs::write(&tmp, &program).map_err(disk)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(disk)?;
        }
        let exe = dir.join(exe_name());
        std::fs::rename(&tmp, &exe).map_err(disk)?;
        std::fs::write(dir.join("arduino-cli.yaml"), config_text(&data_dir.join("tools"))).map_err(disk)?;
        Ok(exe)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Downloads over HTTPS with `ureq` (rustls, with the webpki root certificates).
pub struct HttpDownloader;

impl Downloader for HttpDownloader {
    fn download(
        &self,
        url: &str,
        to: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let mut res = ureq::get(url).call().map_err(|e| e.to_string())?;
        let total = res.body().content_length();
        let mut body = res.body_mut().with_config().limit(512 * 1024 * 1024).reader();
        let mut file = std::fs::File::create(to).map_err(|e| e.to_string())?;
        let mut buf = vec![0; 64 * 1024];
        let mut done = 0u64;
        let mut last = 0u64;
        loop {
            let n = body.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if done - last >= 256 * 1024 {
                last = done;
                progress(done, total);
            }
        }
        progress(done, total);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// Serves files from memory, and remembers what was asked for.
    #[derive(Default)]
    pub struct FakeDownloads {
        pub files: BTreeMap<String, Vec<u8>>,
        pub asked: Mutex<Vec<String>>,
    }

    impl Downloader for FakeDownloads {
        fn download(
            &self,
            url: &str,
            to: &Path,
            progress: &mut dyn FnMut(u64, Option<u64>),
        ) -> Result<(), String> {
            self.asked.lock().unwrap().push(url.to_string());
            let body = self.files.get(url).ok_or_else(|| "404 Not Found".to_string())?;
            progress(body.len() as u64 / 2, Some(body.len() as u64));
            std::fs::write(to, body).map_err(|e| e.to_string())?;
            progress(body.len() as u64, Some(body.len() as u64));
            Ok(())
        }
    }

    /// A tar archive of these files, as `tar` writes it (ustar headers).
    pub fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data) in files {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[100..108].copy_from_slice(b"0000755\0");
            h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
            h[156] = b'0';
            h[257..263].copy_from_slice(b"ustar\0");
            h[148..156].copy_from_slice(b"        ");
            let sum: u32 = h.iter().map(|&b| b as u32).sum();
            h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            out.extend_from_slice(&h);
            out.extend_from_slice(data);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
        out.extend_from_slice(&[0; 1024]);
        out
    }

    pub fn gzip(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        io::Write::write_all(&mut e, data).unwrap();
        e.finish().unwrap()
    }

    pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            io::Write::write_all(&mut w, data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    /// The release for this computer, served by a fake: its archive holds `program` as
    /// arduino-cli, and the checksums list says `sum` (or the archive's real sum).
    pub fn serve(program: &[u8], sum: Option<&str>) -> (FakeDownloads, Release) {
        let release = Release::for_this_computer().expect("a computer Arduino publishes arduino-cli for");
        let files = [(exe_name(), program), ("LICENSE.txt", &b"GPL"[..])];
        let archive = if release.archive.ends_with(".zip") { zip_of(&files) } else { gzip(&tar(&files)) };
        let real: String = Sha256::digest(&archive).iter().map(|b| format!("{b:02x}")).collect();
        let sums = format!(
            "{}  arduino-cli_{v}_Other_64bit.tar.gz\n{}  {}\n",
            "0".repeat(64),
            sum.unwrap_or(&real),
            release.archive,
            v = CLI_VERSION
        );
        let mut d = FakeDownloads::default();
        d.files.insert(release.archive_url.clone(), archive);
        d.files.insert(release.checksums_url.clone(), sums.into_bytes());
        (d, release)
    }

    #[test]
    fn the_release_urls_are_arduinos() {
        let r = Release::for_platform("linux", "aarch64").unwrap();
        assert_eq!(r.archive, "arduino-cli_1.5.1_Linux_ARM64.tar.gz");
        assert_eq!(
            r.archive_url,
            "https://github.com/arduino/arduino-cli/releases/download/v1.5.1/arduino-cli_1.5.1_Linux_ARM64.tar.gz"
        );
        assert_eq!(
            r.checksums_url,
            "https://github.com/arduino/arduino-cli/releases/download/v1.5.1/1.5.1-checksums.txt"
        );
        assert_eq!(
            Release::for_platform("windows", "x86_64").unwrap().archive,
            "arduino-cli_1.5.1_Windows_64bit.zip"
        );
        assert_eq!(
            Release::for_platform("linux", "arm").unwrap().archive,
            "arduino-cli_1.5.1_Linux_ARMv7.tar.gz"
        );
        assert_eq!(Release::for_platform("freebsd", "x86_64"), None);
    }

    #[test]
    fn the_published_sum_is_found_by_file_name() {
        let sums = "abc  x.zip\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA  a.zip\n";
        assert_eq!(published_sha256(sums, "a.zip"), Some("a".repeat(64)));
        assert_eq!(published_sha256(sums, "x.zip"), None, "not a SHA-256");
        assert_eq!(published_sha256(sums, "b.zip"), None);
    }

    #[test]
    fn a_checked_download_is_unpacked_with_its_config() {
        let data = tempfile::tempdir().unwrap();
        let (d, release) = serve(b"the program", None);
        let mut steps = Vec::new();
        let exe = install_cli(&d, data.path(), &release, &mut |s| steps.push(s)).unwrap();
        assert_eq!(exe, faber_dir(data.path()).join(exe_name()));
        assert_eq!(std::fs::read(&exe).unwrap(), b"the program");
        let cfg = std::fs::read_to_string(faber_dir(data.path()).join("arduino-cli.yaml")).unwrap();
        assert!(cfg.contains("arduino15"), "{cfg}");
        assert!(matches!(steps[0], Step::Downloading { .. }));
        assert_eq!(steps[steps.len() - 2..], [Step::Verifying, Step::Unpacking]);
        assert_eq!(*d.asked.lock().unwrap(), [release.checksums_url.clone(), release.archive_url.clone()]);
        assert!(!data.path().join("tools/.download").exists(), "the download is cleaned up");
    }

    #[test]
    fn a_download_that_isnt_the_published_file_installs_nothing() {
        let data = tempfile::tempdir().unwrap();
        let (d, release) = serve(b"evil", Some(&"1".repeat(64)));
        let e = install_cli(&d, data.path(), &release, &mut |_| {}).unwrap_err();
        assert_eq!(e.code, "checksum-mismatch");
        assert!(e.hint.contains("Nothing was installed"));
        assert!(!faber_dir(data.path()).join(exe_name()).exists());
    }

    #[test]
    fn no_published_sum_means_no_install() {
        let data = tempfile::tempdir().unwrap();
        let (mut d, release) = serve(b"x", None);
        d.files.insert(release.checksums_url.clone(), b"nothing here\n".to_vec());
        let e = install_cli(&d, data.path(), &release, &mut |_| {}).unwrap_err();
        assert_eq!(e.code, "checksum-missing");
        assert_eq!(d.asked.lock().unwrap().len(), 1, "the program isn't even downloaded");
    }

    #[test]
    fn being_offline_says_what_to_check() {
        let data = tempfile::tempdir().unwrap();
        let release = Release::for_this_computer().unwrap();
        let e = install_cli(&FakeDownloads::default(), data.path(), &release, &mut |_| {}).unwrap_err();
        assert_eq!(e.code, "download-failed");
        assert!(e.message.contains("404"), "{}", e.message);
        assert!(e.hint.contains("internet"));
    }

    #[test]
    fn tar_files_are_found_by_name() {
        let t = tar(&[("LICENSE.txt", b"l"), ("arduino-cli", b"prog")]);
        assert_eq!(tar_file(&t, "arduino-cli").as_deref(), Some(&b"prog"[..]));
        assert_eq!(tar_file(&t, "missing"), None);
        assert_eq!(tar_file(&t[..600], "arduino-cli"), None, "a cut-off archive");
    }
}
