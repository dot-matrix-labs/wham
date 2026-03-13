#[test]
fn cdp_flow() {
    std::env::set_var("CDP_HEADLESS", "0");
    let result = cdp_runner::run(cdp_runner::Config::default());
    assert!(result.is_ok(), "cdp runner failed: {:?}", result.err());
}
