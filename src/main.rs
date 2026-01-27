use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use zbus::blocking::Connection;
use zbus::zvariant::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut dst_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/mak"));
    dst_dir.push("clawd/screenshots");
    fs::create_dir_all(&dst_dir)?;

    let conn = Connection::session()?;

    // Call Screenshot on the portal; this returns a request object path
    let reply: zbus::zvariant::ObjectPath = conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.Screenshot"),
        "Screenshot",
        &(false, std::collections::HashMap::<&str, Value>::new()),
    )?;

    eprintln!("request object: {}", reply);

    // Wait for the Response signal on that request object
    let start = SystemTime::now();
    let timeout = Duration::from_secs(5);
    let mut found: Option<PathBuf> = None;

    // receive_signal yields incoming signals; iterate until we find the one for our request
    let mut signals = conn.receive_signal()?;
    while let Some(msg) = signals.next() {
        let msg = msg?;
        // Filter signals by interface, member and path
        if let Some(header) = msg.header() {
            if header.interface().map(|i| i.as_str()) == Some("org.freedesktop.portal.Request")
                && header.member().map(|m| m.as_str()) == Some("Response")
                && header.path().map(|p| p.to_string()) == Some(reply.to_string())
            {
                eprintln!("got Response signal");
                // Signal body is (u, a{sv})
                let (code, map): (u32, std::collections::HashMap<String, Value>) = msg.body()?;
                eprintln!("code={} map={:?}", code, map);
                // portal usually returns 'uri' in the results
                if let Some(v) = map.get("uri") {
                    if let Value::Str(s) = v {
                        let path = s.trim_start_matches("file://");
                        let path = urlencoding::decode(path)?.into_owned();
                        let src = PathBuf::from(path);
                        if src.exists() {
                            found = Some(src);
                            break;
                        }
                    }
                }
            }
        }

        if start.elapsed()? > timeout { break; }
    }

    if let Some(src) = found {
        let filename = src.file_name().unwrap_or_else(|| std::ffi::OsStr::new("screenshot.png"));
        let mut dst = dst_dir.clone();
        dst.push(filename);
        fs::copy(&src, &dst)?;
        println!("SAVED:{}", dst.display());
        return Ok(());
    }

    Err("no screenshot found after request".into())
}
