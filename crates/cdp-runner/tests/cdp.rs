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
// Atomic tests — each one starts from a fresh browser session and drives
// to exactly the UI state it wants to verify.
// ---------------------------------------------------------------------------

/// Test 1: App loads and renders a canvas element (accessibility root
/// contains the top-level "GPU Forms UI" label).
#[test]
fn initial_load_renders_canvas() {
    let mut session = match launch_or_skip() {
        Some(s) => s,
        None => return,
    };
    session
        .setup_initial_load()
        .expect("failed to reach initial load state");
    let mut regression_errors = Vec::new();
    session
        .take_screenshot("canvas_initial_load", &mut regression_errors)
        .expect("screenshot failed");
    assert!(
        regression_errors.is_empty(),
        "visual regression(s): {}",
        regression_errors.join(", ")
    );
}

/// Test 2: Navigating to the Dynamic Validation form shows the expected
/// Username and Age fields.
#[test]
fn dynamic_validation_form_loads() {
    let mut session = match launch_or_skip() {
        Some(s) => s,
        None => return,
    };
    session
        .setup_dynamic_form()
        .expect("failed to reach dynamic validation form");
    let mut regression_errors = Vec::new();
    session
        .take_screenshot("dynamic_form_loaded", &mut regression_errors)
        .expect("screenshot failed");
    assert!(
        regression_errors.is_empty(),
        "visual regression(s): {}",
        regression_errors.join(", ")
    );
}

/// Test 3: Typing a valid username ("user1") and age ("18") is reflected in
/// the form fields.
#[test]
fn dynamic_validation_form_accepts_valid_input() {
    let mut session = match launch_or_skip() {
        Some(s) => s,
        None => return,
    };
    session
        .setup_filled_form()
        .expect("failed to reach filled form state");
    let mut regression_errors = Vec::new();
    session
        .take_screenshot("valid_input_filled", &mut regression_errors)
        .expect("screenshot failed");
    assert!(
        regression_errors.is_empty(),
        "visual regression(s): {}",
        regression_errors.join(", ")
    );
}

/// Test 4: Clicking Submit shows the "Submitting…" loading indicator.
#[test]
fn form_submit_shows_submitting_state() {
    let mut session = match launch_or_skip() {
        Some(s) => s,
        None => return,
    };
    session
        .setup_submitting_state()
        .expect("failed to reach submitting state");
    let mut regression_errors = Vec::new();
    session
        .take_screenshot("submitting_state", &mut regression_errors)
        .expect("screenshot failed");
    assert!(
        regression_errors.is_empty(),
        "visual regression(s): {}",
        regression_errors.join(", ")
    );
}

/// Test 5: After a successful submission the confirmation message
/// "Saved successfully." is shown.
#[test]
fn form_submit_success_shows_confirmation() {
    let mut session = match launch_or_skip() {
        Some(s) => s,
        None => return,
    };
    session
        .setup_success_state()
        .expect("failed to reach success state");
    let mut regression_errors = Vec::new();
    session
        .take_screenshot("success_state", &mut regression_errors)
        .expect("screenshot failed");
    assert!(
        regression_errors.is_empty(),
        "visual regression(s): {}",
        regression_errors.join(", ")
    );
}

/// Test 6: Re-submitting after a successful save triggers a server error and
/// the "Server error, rolled back." message is shown.
#[test]
fn form_submit_failure_shows_rollback_message() {
    let mut session = match launch_or_skip() {
        Some(s) => s,
        None => return,
    };
    session
        .setup_rollback_state()
        .expect("failed to reach rollback state");
    let mut regression_errors = Vec::new();
    session
        .take_screenshot("rollback_state", &mut regression_errors)
        .expect("screenshot failed");
    assert!(
        regression_errors.is_empty(),
        "visual regression(s): {}",
        regression_errors.join(", ")
    );
}
