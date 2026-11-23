//! Contract tests for the repository-root GitHub Composite Action.

const ACTION: &str = include_str!("../../action.yml");

#[test]
fn action_declares_all_phase_12d_inputs_and_outputs() {
    for contract in [
        "using: composite",
        "command:",
        "analyze, scan, port or build",
        "path:",
        "to:",
        "loader:",
        "fail-on-risk:",
        "default: high",
        "exit-code:",
        "report-path:",
        "${{ steps.execute.outputs.exit-code }}",
        "${{ steps.execute.outputs.report-path }}",
    ] {
        assert!(
            ACTION.contains(contract),
            "action.yml is missing contract fragment: {contract}"
        );
    }
}

#[test]
fn action_installs_cli_and_publishes_ci_report() {
    for contract in [
        "actions-rust-lang/setup-rust-toolchain@v1",
        "cargo install --path cli --locked",
        "set +e",
        "GITHUB_STEP_SUMMARY",
        "actions/upload-artifact@v4",
        "name: Apply UMMP exit code",
    ] {
        assert!(
            ACTION.contains(contract),
            "action.yml is missing execution fragment: {contract}"
        );
    }
}
