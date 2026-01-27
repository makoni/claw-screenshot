use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use async_std::future::timeout;
use async_std::prelude::StreamExt;
use zbus::zvariant::Value;

fn extract_uri_from_map(map: &HashMap<String, Value>) -> Option<String> {
    // Portal responses vary; try common keys: "uri", or nested under "results" payloads.
    if let Some(v) = map.get("uri") {
        if let Value::Str(s) = v {
            return Some(s.to_string());
        }
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

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut dst_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/mak"));
    dst_dir.push("clawd/screenshots");
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

    let reply: zbus::zvariant::OwnedObjectPath =
        proxy.call("Screenshot", &("", HashMap::<&str, Value>::new())).await?;
    eprintln!("request object: {}", reply);

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
            eprintln!("got Response signal code={} map={:?}", code, map);
            if let Some(s) = extract_uri_from_map(&map) {
                let path = s.trim_start_matches("file://");
                let path = urlencoding::decode(path)?.into_owned();
                let src = PathBuf::from(path);
                if src.exists() {
                    found = Some(src);
                    break;
                }
                eprintln!("decoded path does not exist: {}", src.display());
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    };

    match timeout(Duration::from_secs(10), wait).await {
        Ok(result) => result?,
        Err(_) => eprintln!("timeout waiting for Response signal"),
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
