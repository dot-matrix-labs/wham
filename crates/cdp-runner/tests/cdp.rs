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
