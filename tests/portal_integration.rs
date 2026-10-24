//! End-to-end test: run the real binary against a mock FreeDesktop Screenshot
//! portal served on a private D-Bus session bus.
//!
//! Requires `dbus-daemon` (preinstalled on GitHub's ubuntu-latest runners).
//! The test is skipped when it is not available.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::{OwnedObjectPath, Variant, fdo};

// Object path elements must be valid D-Bus identifiers (no hyphens,
// no leading digits).
const REQUEST_PATH: &str = "/org/freedesktop/portal/desktop/request/_12345/mock_1";

struct MockPortal {
    /// The number of options the Screenshot call carried (None until called).
    received_options: Arc<Mutex<Option<usize>>>,
}

#[zbus::interface(name = "org.freedesktop.portal.Screenshot")]
impl MockPortal {
    /// The portal `Screenshot` method; the mock records the options it was
    /// called with and returns a request object path.
    async fn screenshot(
        &self,
        _parent_window: &str,
        options: HashMap<String, Variant<'_>>,
    ) -> Result<OwnedObjectPath, fdo::Error> {
        *self.received_options.lock().unwrap() = Some(options.len());
        // REQUEST_PATH is a valid object path constant.
        Ok(OwnedObjectPath::try_from(REQUEST_PATH).expect("valid object path"))
    }
}

/// Start a private session bus and return its address and the daemon child.
fn spawn_private_bus(work_dir: &std::path::Path) -> Option<(String, std::process::Child)> {
    let socket_dir = work_dir.join("bus");
    std::fs::create_dir_all(&socket_dir).ok()?;
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
        .arg(format!("--address=unix:dir={}", socket_dir.display()))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let n = reader.read_line(&mut line).ok().unwrap_or(0);
    if n == 0 {
        let _ = child.kill();
        return None;
    }
    Some((line.trim().to_string(), child))
}

#[tokio::test]
async fn screenshot_flow_with_mock_portal() {
    let work = std::env::temp_dir().join(format!(
        "claw-screenshot-integration-{}",
        std::process::id()
    ));

    // The bus daemon must be spawned and read from on a worker thread:
    // the blocking read would stall the current-thread runtime otherwise.
    let bus_work = work.clone();
    let bus = std::thread::spawn(move || spawn_private_bus(&bus_work));
    let (addr, mut daemon) = match bus.join() {
        Ok(Some(bus)) => bus,
        _ => {
            eprintln!("dbus-daemon not available; skipping mock portal test");
            return;
        }
    };

    // The fake screenshot the mock portal will point at, and the destination
    // directory the binary is asked to save into.
    let shot = work.join("Screenshot-mock.png");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(&shot, b"\x89PNG fake screenshot payload").unwrap();
    let dst_dir = work.join("dst");
    std::fs::create_dir_all(&dst_dir).unwrap();
    let uri = format!("file://{}", shot.display());

    // The mock portal owns org.freedesktop.portal.Desktop on the private bus.
    let received_options: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
    let portal = MockPortal {
        received_options: received_options.clone(),
    };
    let conn = zbus::connection::Builder::address(addr.as_str())
        .name("org.freedesktop.portal.Desktop")
        .serve_at("/org/freedesktop/portal/desktop", portal)
        .build()
        .await
        .expect("mock portal connection");

    // Once the binary called Screenshot, emit the Response signal on the
    // request object (after a pause, so the client has time to register its
    // match rule first).
    let responder_options = received_options.clone();
    let responder = tokio::spawn(async move {
        let mut called = false;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if responder_options.lock().unwrap().is_some() {
                called = true;
                break;
            }
        }
        assert!(called, "the binary never called the Screenshot method");
        tokio::time::sleep(Duration::from_millis(500)).await;
        let results: HashMap<String, Variant<'static>> =
            HashMap::from([("uri".to_string(), Variant::new(uri))]);
        let emitter = zbus::object_server::SignalEmitter::new(&conn, REQUEST_PATH)
            .expect("build signal emitter");
        emitter
            .emit(
                "org.freedesktop.portal.Request",
                "Response",
                &(0u32, results),
            )
            .await
            .expect("emit Response signal");
    });

    let dst_dir_env = dst_dir.clone();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_claw-screenshot"))
            .env("DBUS_SESSION_BUS_ADDRESS", &addr)
            .env("CLAW_SCREENSHOT_DIR", &dst_dir_env)
            .env("CLAW_SCREENSHOT_RESPONSE_TIMEOUT", "30")
            .env("CLAW_SCREENSHOT_FILE_TIMEOUT", "30")
            .output()
    })
    .await
    .expect("run the binary")
    .expect("spawn the binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "binary failed (exit {:?}): {stderr}",
        output.status.code()
    );

    let expected = dst_dir.join("Screenshot-mock.png");
    assert!(
        stdout.contains(&format!("Saved screenshot: {}", expected.display())),
        "unexpected stdout: {stdout}"
    );
    assert!(
        expected.exists(),
        "screenshot was not saved to the destination"
    );

    // The mock recorded that the binary sent no options.
    let options_count = received_options.lock().unwrap().unwrap_or_default();
    assert_eq!(
        options_count, 0,
        "unexpected options sent to Screenshot: {options_count}"
    );

    let _ = responder.await;
    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = std::fs::remove_dir_all(&work);
}
