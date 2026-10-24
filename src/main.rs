use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use log::debug;
use tokio::time::{sleep, timeout};
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{OwnedObjectPath, Variant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScreenshotTarget {
    /// A window picked by the portal (target bit 2, interface v3).
    Window,
    /// An area picked by the portal (target bit 4, interface v3).
    Area,
    /// The currently active window (target bit 8, interface v3).
    ActiveWindow,
}

impl ScreenshotTarget {
    /// The `target` bit this selection maps to per the Screenshot spec v3.
    fn bit(self) -> u32 {
        match self {
            ScreenshotTarget::Window => 2,
            ScreenshotTarget::Area => 4,
            ScreenshotTarget::ActiveWindow => 8,
        }
    }

    /// Human-readable name for error messages.
    fn label(self) -> &'static str {
        match self {
            ScreenshotTarget::Window => "window",
            ScreenshotTarget::Area => "area",
            ScreenshotTarget::ActiveWindow => "active window",
        }
    }
}

/// The largest delay (in seconds) accepted for `--delay`.
const MAX_DELAY_SECS: u64 = 3600;

#[derive(Debug, Default, PartialEq, Eq)]
struct CliArgs {
    help: bool,
    version: bool,
    target: Option<ScreenshotTarget>,
    delay: Option<u64>,
}

/// Parse a `--delay` value in seconds.
fn parse_delay(value: &str) -> Result<u64, String> {
    match value.parse::<u64>() {
        Ok(secs) if secs <= MAX_DELAY_SECS => Ok(secs),
        _ => Err(format!(
            "invalid delay: {value:?} (expected seconds between 0 and {MAX_DELAY_SECS})"
        )),
    }
}

fn parse_cli_args(args: &[String]) -> Result<CliArgs, String> {
    let mut parsed = CliArgs::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => parsed.help = true,
            "-V" | "--version" => parsed.version = true,
            "--delay" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("--delay requires a value (seconds)".to_string());
                };
                // A second --delay overrides the first.
                parsed.delay = Some(parse_delay(value)?);
            }
            arg if let Some(value) = arg.strip_prefix("--delay=") => {
                parsed.delay = Some(parse_delay(value)?);
            }
            flag => {
                let target = match flag {
                    "--window" => Some(ScreenshotTarget::Window),
                    "--area" => Some(ScreenshotTarget::Area),
                    "--active-window" => Some(ScreenshotTarget::ActiveWindow),
                    _ => return Err(format!("unknown argument: {flag}")),
                };
                if parsed.target.is_some() {
                    return Err(
                        "only one of --window, --area, --active-window is allowed".to_string()
                    );
                }
                parsed.target = target;
            }
        }
        i += 1;
    }
    Ok(parsed)
}

fn print_help(program: &str) {
    println!("Claw Screenshot");
    println!();
    println!("Take a desktop screenshot via the FreeDesktop Screenshot portal and save it");
    println!("to your Pictures folder.");
    println!();
    println!("Usage:");
    println!("  {program} [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -h, --help           Show this help message and exit");
    println!("  -V, --version        Print version information and exit");
    println!("  --window             Capture a window selected by the portal");
    println!("  --area               Capture an area selected by the portal");
    println!("  --active-window      Capture the currently active window");
    println!("  --delay <seconds>    Wait this long before taking the screenshot (0..3600)");
    println!();
    println!("Target capture requires a portal that implements the Screenshot interface");
    println!("version 3 (it advertises the AvailableTargets property); otherwise the");
    println!("tool reports that the target is not supported.");
    println!();
    println!("Environment:");
    println!("  CLAW_SCREENSHOT_DIR  Save screenshots into this directory (default: ~/Pictures)");
    println!(
        "  CLAW_SCREENSHOT_RESPONSE_TIMEOUT  Seconds to wait for the portal response (default: 10)"
    );
    println!(
        "  CLAW_SCREENSHOT_FILE_TIMEOUT      Seconds to wait for the file to be ready (default: 10)"
    );
    println!("  RUST_LOG             Logging details, e.g. RUST_LOG=debug");
    println!();
    println!("Note: the desktop portal grants screenshot permission on first run (or may");
    println!("deny it for terminal launches); see the README for details.");
}

fn print_version() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

fn extract_uri_from_map(map: &HashMap<String, Variant>) -> Option<String> {
    // The Screenshot portal returns the saved file as `uri` (a string) in the
    // results vardict of the Response signal.
    if let Some(Variant::Str(s)) = map.get("uri") {
        return Some(s.to_string());
    }

    // Defensive fallback for portal builds that nest the file URI in other
    // variants: scan the values for a file:// string.
    fn find_file_uri(v: &Variant) -> Option<String> {
        match v {
            Variant::Str(s) => s.strip_prefix("file://").map(|_| s.to_string()),
            // Values nested in a VARIANT (e.g. elements of arrays of type v)
            // need unwrapping to reach the inner string.
            Variant::Variant(inner) => find_file_uri(inner),
            Variant::Dict(d) => d
                .iter()
                .find_map(|(k, val)| find_file_uri(k).or_else(|| find_file_uri(val))),
            Variant::Array(a) => a.inner().iter().find_map(find_file_uri),
            Variant::Structure(s) => s.fields().iter().find_map(find_file_uri),
            _ => None,
        }
    }

    map.values().find_map(find_file_uri)
}

async fn wait_for_file_ready(
    path: &Path,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let mut last_size: Option<u64> = None;
    loop {
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len();
            if size > 0 {
                if Some(size) == last_size {
                    return Ok(());
                }
                last_size = Some(size);
            }
        }
        if start.elapsed() >= timeout {
            return Err(format!(
                "screenshot file not ready after {}s (CLAW_SCREENSHOT_FILE_TIMEOUT): {}",
                timeout.as_secs(),
                path.display()
            )
            .into());
        }
        sleep(Duration::from_millis(200)).await;
    }
}

/// Parse a timeout in seconds; `0` is rejected as a footgun.
fn parse_timeout_secs(value: &str) -> Option<Duration> {
    value
        .parse::<u64>()
        .ok()
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
}

/// Read a timeout in seconds from the environment, falling back to `default`.
fn env_timeout(name: &str, default: Duration) -> Duration {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => match parse_timeout_secs(&v) {
            Some(d) => d,
            None => {
                debug!("ignoring invalid {name}={v:?}; using default");
                default
            }
        },
        _ => default,
    }
}

/// Build the options vardict for the Screenshot call, checking that the
/// portal advertises the requested capture target (interface v3).
async fn build_screenshot_options(
    proxy: &zbus::Proxy<'_>,
    target: Option<ScreenshotTarget>,
) -> Result<HashMap<String, Variant<'static>>, Box<dyn std::error::Error>> {
    let mut options: HashMap<String, Variant<'static>> = HashMap::new();
    let Some(target) = target else {
        return Ok(options);
    };
    let bit = target.bit();
    // `target` and `AvailableTargets` were added in interface version 3;
    // older portals simply do not expose the property.
    let available = match proxy.get_property::<u32>("AvailableTargets").await {
        Ok(v) => v,
        Err(err) => {
            return Err(format!(
                "{} capture is not supported by the portal: could not read the \
                 AvailableTargets property (requires Screenshot interface \
                 version 3): {err}",
                target.label()
            )
            .into());
        }
    };
    if available == 0 {
        return Err(format!(
            "{} capture is not supported by the portal: it advertises no \
             screenshot targets",
            target.label()
        )
        .into());
    }
    if available & bit != bit {
        return Err(format!(
            "{} capture is not supported by the portal: AvailableTargets = {available:#b}",
            target.label()
        )
        .into());
    }
    // A single target value, not a bitmask, per the spec.
    options.insert("target".to_string(), Variant::new(bit));
    Ok(options)
}

async fn run(target: Option<ScreenshotTarget>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Destination directory can be configured via environment variable CLAW_SCREENSHOT_DIR
    // Default: ~/Pictures
    let (dst_dir, move_requested) = match std::env::var("CLAW_SCREENSHOT_DIR") {
        Ok(dir) if !dir.trim().is_empty() => (PathBuf::from(dir), true),
        _ => {
            let p = dirs::picture_dir()
                .or_else(|| {
                    dirs::home_dir().map(|mut h| {
                        h.push("Pictures");
                        h
                    })
                })
                .ok_or("could not determine picture directory; set CLAW_SCREENSHOT_DIR")?;
            (p, false)
        }
    };
    fs::create_dir_all(&dst_dir)?;

    let conn = zbus::Connection::session().await?;

    // Call Screenshot on the portal; this returns a request object path
    // Try using a Proxy to call the Screenshot method which is often simpler
    let proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )
    .await?;

    let options = build_screenshot_options(&proxy, target).await?;

    let reply: OwnedObjectPath = proxy.call("Screenshot", &("", options)).await?;
    debug!("request object: {}", reply);

    let response_timeout = env_timeout("CLAW_SCREENSHOT_RESPONSE_TIMEOUT", Duration::from_secs(10));
    let file_timeout = env_timeout("CLAW_SCREENSHOT_FILE_TIMEOUT", Duration::from_secs(10));

    // Wait for the Response signal on that request object
    let mut found: Option<PathBuf> = None;
    let wait = async {
        let request_proxy = zbus::Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            reply.as_str(),
            "org.freedesktop.portal.Request",
        )
        .await?;
        let mut stream = request_proxy.receive_signal("Response").await?;
        while let Some(msg) = stream.next().await {
            let body = msg.body();
            let (code, map): (u32, HashMap<String, Variant>) = body.deserialize()?;
            debug!("got Response signal code={} map={:?}", code, map);
            if let Some(s) = extract_uri_from_map(&map) {
                let path = s.trim_start_matches("file://");
                let path = urlencoding::decode(path)?.into_owned();
                let src = PathBuf::from(path);
                found = Some(src);
                break;
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    };

    let src = match timeout(response_timeout, wait).await {
        Ok(Ok(())) => found.ok_or("portal response contained no screenshot file")?,
        Ok(Err(err)) => return Err(err),
        Err(_) => {
            return Err(format!(
                "timed out after {}s waiting for the portal response (CLAW_SCREENSHOT_RESPONSE_TIMEOUT)",
                response_timeout.as_secs()
            )
            .into())
        }
    };

    wait_for_file_ready(&src, file_timeout).await?;

    let filename = src
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("screenshot.png"));
    let mut dst = dst_dir.clone();
    dst.push(filename);
    if src != dst {
        if move_requested {
            if let Err(err) = fs::rename(&src, &dst) {
                if fs::copy(&src, &dst).is_ok() {
                    fs::remove_file(&src)?;
                } else {
                    return Err(err.into());
                }
            }
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    debug!("saved screenshot to {}", dst.display());
    Ok(dst)
}

/// Pause for `secs` before taking the screenshot, printing a countdown so
/// the user can get ready. Goes to stderr to keep stdout a clean "Saved
/// screenshot: <path>" line for scripting.
async fn wait_delay(secs: u64) {
    if secs == 0 {
        return;
    }
    eprintln!("Waiting {secs}s before taking the screenshot (Ctrl+C to cancel)…");
    for remaining in (1..=secs).rev() {
        eprintln!("{remaining}");
        sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    // initialize logging (use RUST_LOG to control level, e.g. RUST_LOG=info)
    let _ = env_logger::try_init();

    let argv: Vec<String> = std::env::args().collect();
    let program = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "claw-screenshot".to_string());
    let args = match parse_cli_args(&argv[1..]) {
        Ok(args) => args,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("Try '{program} --help' for usage.");
            return std::process::ExitCode::from(2);
        }
    };
    if args.help {
        print_help(&program);
        return std::process::ExitCode::SUCCESS;
    }
    if args.version {
        print_version();
        return std::process::ExitCode::SUCCESS;
    }

    if let Some(secs) = args.delay {
        wait_delay(secs).await;
    }

    match run(args.target).await {
        Ok(dst) => {
            println!("Saved screenshot: {}", dst.display());
            std::process::ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("claw-screenshot: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
