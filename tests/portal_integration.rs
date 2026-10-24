//! End-to-end tests: run the real binary against a mock FreeDesktop Screenshot
//! portal served on a private D-Bus session bus.
//!
//! Requires `dbus-daemon` (preinstalled on GitHub's ubuntu-latest runners).
//! The tests are skipped when it is not available.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::{OwnedObjectPath, Variant, fdo};

// Object path elements must be valid D-Bus identifiers (no hyphens,
// no leading digits).
const REQUEST_PATH: &str = "/org/freedesktop/portal/desktop/request/_12345/mock_1";

/// All v3 target bits: Screen | Window | Area | ActiveWindow.
const ALL_TARGETS: u32 = 0b1_111;

struct MockPortal {
    /// The v3 `AvailableTargets` bitmask the mock advertises; `None`
    /// emulates a pre-v3 portal that has no such property at all.
    available_targets: Option<u32>,
    /// Whether the binary called the Screenshot method.
    called: Arc<AtomicBool>,
    /// The `target` option the Screenshot call carried (None until called).
    received_target: Arc<Mutex<Option<u32>>>,
    /// The number of options the Screenshot call carried (0 until called).
    received_option_count: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.freedesktop.portal.Screenshot")]
impl MockPortal {
    /// The v3 `AvailableTargets` property (a D-Bus error when absent).
    #[zbus(property, name = "AvailableTargets")]
    fn available_targets(&self) -> Result<u32, fdo::Error> {
        self.available_targets
            .ok_or_else(|| fdo::Error::NotSupported("not a v3 portal".to_string()))
    }

    /// The portal `Screenshot` method; the mock records the options it was
    /// called with and returns a request object path.
    async fn screenshot(
        &self,
        _parent_window: &str,
        options: HashMap<String, Variant<'_>>,
    ) -> Result<OwnedObjectPath, fdo::Error> {
        self.called.store(true, Ordering::SeqCst);
        self.received_option_count
            .store(options.len(), Ordering::SeqCst);
        if let Some(Variant::U32(target)) = options.get("target") {
            *self.received_target.lock().unwrap() = Some(*target);
        }
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

/// Owns the private bus daemon and its working directory; tears both down
/// on drop so a failing test cannot leak a process or a temp dir.
struct PrivateBus {
    addr: String,
    work: std::path::PathBuf,
    daemon: std::process::Child,
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

struct FlowResult {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    called: bool,
    target: Option<u32>,
    option_count: usize,
    /// Whether the saved screenshot existed at the destination (checked
    /// before the working directory is torn down).
    saved_exists: bool,
    /// The destination directory (removed when the bus is dropped, kept for
    /// asserting on the binary's stdout).
    dst_dir: PathBuf,
}

/// Unique per-run suffix for the working directory: the tests share one
/// process (hence one PID), so a counter guarantees uniqueness even if two
/// runs sample the clock within the same nanosecond.
static WORK_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Run the binary against a mock portal that advertises `available_targets`
/// (`None` emulates a pre-v3 portal without the property) and report what
/// happened. Returns `None` when `dbus-daemon` is missing.
async fn run_flow(available_targets: Option<u32>, cli_args: &[&str]) -> Option<FlowResult> {
    let seq = WORK_COUNTER.fetch_add(1, Ordering::SeqCst);
    let work = std::env::temp_dir().join(format!(
        "claw-screenshot-integration-{}-{}",
        std::process::id(),
        seq
    ));

    // The bus daemon must be spawned and read from on a worker thread:
    // the blocking read would stall the current-thread runtime otherwise.
    let bus_work = work.clone();
    let bus = std::thread::spawn(move || spawn_private_bus(&bus_work));
    let bus = match bus.join() {
        Ok(Some((addr, daemon))) => PrivateBus { addr, work, daemon },
        _ => {
            eprintln!("dbus-daemon not available; skipping mock portal test");
            let _ = std::fs::remove_dir_all(&work);
            return None;
        }
    };
    let addr = bus.addr.clone();

    // The fake screenshot the mock portal will point at, and the destination
    // directory the binary is asked to save into.
    let work = bus.work.clone();
    let shot = work.join("Screenshot-mock.png");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(&shot, b"\x89PNG fake screenshot payload").unwrap();
    let dst_dir = work.join("dst");
    std::fs::create_dir_all(&dst_dir).unwrap();
    let uri = format!("file://{}", shot.display());

    // The mock portal owns org.freedesktop.portal.Desktop on the private bus.
    let called = Arc::new(AtomicBool::new(false));
    let received_target: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
    let received_option_count = Arc::new(AtomicUsize::new(0));
    let portal = MockPortal {
        available_targets,
        called: called.clone(),
        received_target: received_target.clone(),
        received_option_count: received_option_count.clone(),
    };
    let conn = zbus::connection::Builder::address(addr.as_str())
        .name("org.freedesktop.portal.Desktop")
        .serve_at("/org/freedesktop/portal/desktop", portal)
        .build()
        .await
        .expect("mock portal connection");

    // Once the binary called Screenshot, emit the Response signal on the
    // request object (after a pause, so the client has time to register its
    // match rule first). Exits early if the binary gave up before calling.
    let responder_called = called.clone();
    let binary_done = Arc::new(AtomicBool::new(false));
    let responder_done = binary_done.clone();
    let responder = tokio::spawn(async move {
        let mut was_called = false;
        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if responder_called.load(Ordering::SeqCst) {
                was_called = true;
                break;
            }
            if responder_done.load(Ordering::SeqCst) {
                break;
            }
        }
        if !was_called {
            return;
        }
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
    let cli_args: Vec<String> = cli_args.iter().map(|s| s.to_string()).collect();
    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_claw-screenshot"));
        cmd.env("DBUS_SESSION_BUS_ADDRESS", &addr)
            .env("CLAW_SCREENSHOT_DIR", &dst_dir_env)
            .env("CLAW_SCREENSHOT_RESPONSE_TIMEOUT", "30")
            .env("CLAW_SCREENSHOT_FILE_TIMEOUT", "30");
        for arg in cli_args {
            cmd.arg(arg);
        }
        cmd.output()
    })
    .await
    .expect("run the binary")
    .expect("spawn the binary");
    binary_done.store(true, Ordering::SeqCst);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();
    let was_called = called.load(Ordering::SeqCst);
    let target = *received_target.lock().unwrap();
    let option_count = received_option_count.load(Ordering::SeqCst);
    // Check the saved file before dropping the bus, which removes the dir.
    let saved_exists = dst_dir.join("Screenshot-mock.png").exists();

    if let Err(join) = responder.await {
        eprintln!("responder task failed: {join:?}");
    }
    drop(bus);

    Some(FlowResult {
        exit_code,
        stdout,
        stderr,
        called: was_called,
        target,
        option_count,
        saved_exists,
        dst_dir,
    })
}

/// Run a scenario against the mock portal; skips (returns without failing)
/// when `dbus-daemon` is not available, otherwise runs `assert_fn` on the
/// outcome.
async fn run_scenario(
    available_targets: Option<u32>,
    cli_args: &[&str],
    mut assert_fn: impl FnMut(FlowResult),
) {
    let Some(r) = run_flow(available_targets, cli_args).await else {
        eprintln!("dbus-daemon not available; skipping mock portal test");
        return;
    };
    assert_fn(r);
}

#[tokio::test]
async fn screenshot_flow_with_mock_portal() {
    run_scenario(Some(ALL_TARGETS), &[], |r| {
        assert!(
            r.exit_code.is_none_or(|c| c == 0),
            "binary failed (exit {:?}): {}",
            r.exit_code,
            r.stderr
        );

        let expected = r.dst_dir.join("Screenshot-mock.png");
        assert!(
            r.stdout
                .contains(&format!("Saved screenshot: {}", expected.display())),
            "unexpected stdout: {}",
            r.stdout
        );
        assert!(
            r.saved_exists,
            "screenshot was not saved to the destination"
        );

        // No target was requested: the mock must have seen no options at all.
        assert!(r.called, "the binary never called the Screenshot method");
        assert_eq!(r.target, None, "unexpected target sent");
        assert_eq!(
            r.option_count, 0,
            "unexpected options sent to Screenshot: {}",
            r.option_count
        );
    })
    .await;
}

#[tokio::test]
async fn screenshot_with_window_target() {
    run_scenario(Some(ALL_TARGETS), &["--window"], |r| {
        assert!(
            r.exit_code.is_none_or(|c| c == 0),
            "binary failed (exit {:?}): {}",
            r.exit_code,
            r.stderr
        );

        let expected = r.dst_dir.join("Screenshot-mock.png");
        assert!(
            r.stdout
                .contains(&format!("Saved screenshot: {}", expected.display())),
            "unexpected stdout: {}",
            r.stdout
        );
        assert!(
            r.saved_exists,
            "screenshot was not saved to the destination"
        );

        // The binary must have sent the Window target bit (2) as a single
        // value, and nothing else.
        assert!(r.called, "the binary never called the Screenshot method");
        assert_eq!(r.target, Some(2), "unexpected target sent");
        assert_eq!(
            r.option_count, 1,
            "unexpected number of options sent to Screenshot: {}",
            r.option_count
        );
    })
    .await;
}

#[tokio::test]
async fn screenshot_with_active_window_target() {
    run_scenario(Some(ALL_TARGETS), &["--active-window"], |r| {
        assert!(
            r.exit_code.is_none_or(|c| c == 0),
            "binary failed (exit {:?}): {}",
            r.exit_code,
            r.stderr
        );
        assert!(
            r.saved_exists,
            "screenshot was not saved to the destination"
        );

        // The binary must have sent the ActiveWindow target bit (8).
        assert!(r.called, "the binary never called the Screenshot method");
        assert_eq!(r.target, Some(8), "unexpected target sent");
        assert_eq!(
            r.option_count, 1,
            "unexpected number of options sent to Screenshot: {}",
            r.option_count
        );
    })
    .await;
}

#[tokio::test]
async fn unsupported_target_reports_error() {
    // The portal advertises Screen (1) only; --area (bit 4) must be refused
    // before any Screenshot call is made.
    run_scenario(Some(0b0001), &["--area"], |r| {
        assert_eq!(
            r.exit_code,
            Some(1),
            "binary should exit 1 for an unsupported target, stderr: {}",
            r.stderr
        );
        assert!(
            r.stderr.contains("area capture is not supported"),
            "unexpected stderr: {}",
            r.stderr
        );
        assert!(!r.called, "the binary called Screenshot despite no support");
    })
    .await;
}

#[tokio::test]
async fn v2_portal_without_property_reports_error() {
    // A pre-v3 portal has no AvailableTargets property at all; the binary
    // must fail with a clear error and never call Screenshot.
    run_scenario(None, &["--window"], |r| {
        assert_eq!(
            r.exit_code,
            Some(1),
            "binary should exit 1 for a v2 portal, stderr: {}",
            r.stderr
        );
        assert!(
            r.stderr.contains("window capture is not supported"),
            "unexpected stderr: {}",
            r.stderr
        );
        assert!(
            r.stderr.contains("AvailableTargets"),
            "error should mention the missing property, stderr: {}",
            r.stderr
        );
        assert!(!r.called, "the binary called Screenshot despite no support");
    })
    .await;
}
