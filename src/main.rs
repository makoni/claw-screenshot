use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use log::debug;
use tokio::time::{sleep, timeout};
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{OwnedObjectPath, Variant};

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run,
    Help,
    Version,
}

fn parse_cli_args(args: &[String]) -> Result<CliAction, String> {
    if args.is_empty() {
        return Ok(CliAction::Run);
    }
    if args.len() == 1 && matches!(args[0].as_str(), "-h" | "--help") {
        return Ok(CliAction::Help);
    }
    if args.len() == 1 && matches!(args[0].as_str(), "-V" | "--version") {
        return Ok(CliAction::Version);
    }
    Err(format!("unknown arguments: {}", args.join(" ")))
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
            // Elements of dynamic arrays are wrapped in a VARIANT.
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
            return Err(format!("screenshot file not ready: {}", path.display()).into());
        }
        sleep(Duration::from_millis(200)).await;
    }
}

fn parse_timeout_secs(value: &str) -> Option<Duration> {
    value.parse::<u64>().ok().map(Duration::from_secs)
}

/// Read a timeout in seconds from the environment, falling back to `default`.
fn env_timeout(name: &str, default: Duration) -> Duration {
    std::env::var(name)
        .ok()
        .and_then(|v| parse_timeout_secs(&v))
        .unwrap_or(default)
}

async fn run() -> Result<PathBuf, Box<dyn std::error::Error>> {
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

    let reply: OwnedObjectPath = proxy
        .call("Screenshot", &("", HashMap::<&str, Variant>::new()))
        .await?;
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    // initialize logging (use RUST_LOG to control level, e.g. RUST_LOG=info)
    let _ = env_logger::try_init();

    let argv: Vec<String> = std::env::args().collect();
    let program = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "claw-screenshot".to_string());
    match parse_cli_args(&argv[1..]) {
        Ok(CliAction::Help) => {
            print_help(&program);
            return std::process::ExitCode::SUCCESS;
        }
        Ok(CliAction::Version) => {
            print_version();
            return std::process::ExitCode::SUCCESS;
        }
        Ok(CliAction::Run) => {}
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("Try '{program} --help' for usage.");
            return std::process::ExitCode::from(2);
        }
    }

    match run().await {
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
