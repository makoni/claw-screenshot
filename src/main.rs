use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use zbus::blocking::Connection;
use zbus::zvariant::Value;

fn extract_uri_from_map(map: &std::collections::HashMap<String, Value>) -> Option<String> {
    // Portal responses vary; try common keys: "uri", or nested under "results" payloads.
    if let Some(v) = map.get("uri") {
        if let Value::Str(s) = v { return Some(s.to_string()); }
    }
    // Some portals return "results" -> a{sv} inside the map under "results"
    if let Some(v) = map.get("results") {
        if let Value::Dict(d) = v {
            // Dict is (Signature, Vec<(Value, Value)>) in zbus representation; try to parse roughly
            // Fallback: stringify debug and search for file://
            let s = format!("{:?}", d);
            if let Some(idx) = s.find("file://") {
                let tail = &s[idx..];
                if let Some(end) = tail.find('"') { return Some(tail[..end].to_string()); }
                return Some(tail.to_string());
            }
        }
    }
    None
}

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
    let timeout = Duration::from_secs(10);
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
                // Try to extract uri more robustly
                if let Some(s) = extract_uri_from_map(&map) {
                    let path = s.trim_start_matches("file://");
                    let path = urlencoding::decode(path)?.into_owned();
                    let src = PathBuf::from(path);
                    if src.exists() {
                        found = Some(src);
                        break;
                    } else {
                        eprintln!("decoded path does not exist: {}", src.display());
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
