/// Helper: launch a browser session from `Config::default()` and return it.
/// If Chrome is not installed the test is skipped (returns `None`).
fn launch_or_skip() -> Option<cdp_runner::BrowserSession> {
    match cdp_runner::BrowserSession::launch(&cdp_runner::Config::default()) {
        Ok(session) => Some(session),
        Err(ref e) if e == "chrome binary not found" => {
            eprintln!("SKIP: no Chrome/Chromium binary found, skipping CDP integration test");
            None
        }
        Err(e) => {
            panic!("browser session launch failed: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Backwards-compatible monolithic test
// ---------------------------------------------------------------------------

#[test]
fn cdp_flow() {
    let result = cdp_runner::run(cdp_runner::Config::default());
    if let Err(ref e) = result {
        if e == "chrome binary not found" {
            eprintln!("SKIP: no Chrome/Chromium binary found, skipping CDP integration test");
            return;
        }
    }
    assert!(result.is_ok(), "cdp runner failed: {:?}", result.err());
}

// ---------------------------------------------------------------------------
// Atomic tests — each drives to a specific UI state independently.
// ---------------------------------------------------------------------------

#[test]
fn initial_load_renders_canvas() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.setup_initial_load().expect("failed to reach initial load state");
    let mut errs = Vec::new();
    session.take_screenshot("canvas_initial_load", &mut errs).expect("screenshot failed");
    assert!(errs.is_empty(), "visual regression(s): {}", errs.join(", "));
}

#[test]
fn dynamic_validation_form_loads() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.setup_dynamic_form().expect("failed to reach dynamic validation form");
    let mut errs = Vec::new();
    session.take_screenshot("dynamic_form_loaded", &mut errs).expect("screenshot failed");
    assert!(errs.is_empty(), "visual regression(s): {}", errs.join(", "));
}

#[test]
fn dynamic_validation_form_accepts_valid_input() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.setup_filled_form().expect("failed to reach filled form state");
    let mut errs = Vec::new();
    session.take_screenshot("valid_input_filled", &mut errs).expect("screenshot failed");
    assert!(errs.is_empty(), "visual regression(s): {}", errs.join(", "));
}

#[test]
fn form_submit_shows_submitting_state() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.setup_submitting_state().expect("failed to reach submitting state");
    let mut errs = Vec::new();
    session.take_screenshot("submitting_state", &mut errs).expect("screenshot failed");
    assert!(errs.is_empty(), "visual regression(s): {}", errs.join(", "));
}

#[test]
fn form_submit_success_shows_confirmation() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.setup_success_state().expect("failed to reach success state");
    let mut errs = Vec::new();
    session.take_screenshot("success_state", &mut errs).expect("screenshot failed");
    assert!(errs.is_empty(), "visual regression(s): {}", errs.join(", "));
}

#[test]
fn form_submit_failure_shows_rollback_message() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.setup_rollback_state().expect("failed to reach rollback state");
    let mut errs = Vec::new();
    session.take_screenshot("rollback_state", &mut errs).expect("screenshot failed");
    assert!(errs.is_empty(), "visual regression(s): {}", errs.join(", "));
}

// ---------------------------------------------------------------------------
// Scenario tests — navigate to ?scenario= URL, verify form renders.
// ---------------------------------------------------------------------------

#[test]
fn sign_in_form_loads() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.navigate_to_scenario("sign-in");
    let mut errs = Vec::new();
    session.take_screenshot("sign_in_loaded", &mut errs).expect("screenshot failed");
}

#[test]
fn checkout_form_loads() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.navigate_to_scenario("checkout");
    let mut errs = Vec::new();
    session.take_screenshot("checkout_loaded", &mut errs).expect("screenshot failed");
}

#[test]
fn notifications_form_loads() {
    let mut session = match launch_or_skip() { Some(s) => s, None => return };
    session.navigate_to_scenario("notifications");
    let mut errs = Vec::new();
    session.take_screenshot("notifications_loaded", &mut errs).expect("screenshot failed");
}
