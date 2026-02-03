use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_std::future::timeout;
use async_std::prelude::StreamExt;
use log::{debug, error};
use zbus::zvariant::Value;

fn extract_uri_from_map(map: &HashMap<String, Value>) -> Option<String> {
    // Portal responses vary; try common keys: "uri", or nested under "results" payloads.
    // Use a recursive scan of zvariant::Value instead of relying on debug output.
    fn find_file_uri_in_value(v: &Value) -> Option<String> {
        // quick debug-string fallback: some portal shapes are nested; search debug output too
        let dbg = format!("{:?}", v);
        if let Some(idx) = dbg.find("file://") {
            let tail = &dbg[idx..];
            if let Some(end) = tail.find('"') {
                return Some(tail[..end].to_string());
            }
            return Some(tail.to_string());
        }
        match v {
            Value::Str(s) => {
                if s.starts_with("file://") || s.contains("file://") {
                    return Some(s.to_string());
                }
                None
            }
            Value::Dict(d) => {
                // iterate dict entries
                for (k, val) in d.iter() {
                    if let Some(u) = find_file_uri_in_value(k) {
                        return Some(u);
                    }
                    if let Some(u) = find_file_uri_in_value(val) {
                        return Some(u);
                    }
                }
                None
            }
            Value::Array(a) => {
                for item in a.inner() {
                    if let Some(u) = find_file_uri_in_value(item) {
                        return Some(u);
                    }
                }
                None
            }
            Value::Structure(s) => {
                for item in s.fields() {
                    if let Some(u) = find_file_uri_in_value(item) {
                        return Some(u);
                    }
                }
                None
            }
            _ => None,
        }
    }

    if let Some(Value::Str(s)) = map.get("uri") {
        return Some(s.to_string());
    }

    if let Some(v) = map.get("results")
        && let Some(u) = find_file_uri_in_value(v)
    {
        return Some(u);
    }

    for v in map.values() {
        if let Some(u) = find_file_uri_in_value(v) {
            return Some(u);
        }
    }

    None
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
        async_std::task::sleep(Duration::from_millis(200)).await;
    }
}
#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize logging (use RUST_LOG to control level, e.g. RUST_LOG=info)
    let _ = env_logger::try_init();

    // Destination directory can be configured via environment variable CLAW_SCREENSHOT_DIR
    // Default: ~/Pictures
    let (dst_dir, move_requested) = match std::env::var("CLAW_SCREENSHOT_DIR") {
        Ok(dir) if !dir.trim().is_empty() => (PathBuf::from(dir), true),
        _ => {
            let p = dirs::picture_dir().unwrap_or_else(|| {
                let mut home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/mak"));
                home.push("Pictures");
                home
            });
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

    let reply: zbus::zvariant::OwnedObjectPath = proxy
        .call("Screenshot", &("", HashMap::<&str, Value>::new()))
        .await?;
    debug!("request object: {}", reply);

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
            let (code, map): (u32, HashMap<String, Value>) = body.deserialize()?;
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

    match timeout(Duration::from_secs(10), wait).await {
        Ok(result) => result?,
        Err(_) => error!("timeout waiting for Response signal"),
    }

    if let Some(src) = found {
        wait_for_file_ready(&src, Duration::from_secs(10)).await?;
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
        println!("Saved screenshot: {}", dst.display());
        return Ok(());
    }

    Err("no screenshot found after request".into())
}

#[cfg(test)]
mod tests;
