use std::process::Command;
#[test]
fn help_and_version_need_neither_terminal_nor_audio() {
    let exe = env!("CARGO_BIN_EXE_shr-fx");
    for (arg, expected) in [("--help", "--data-root"), ("--version", "shr-fx 0.1.0")] {
        let output = Command::new(exe).arg(arg).output().unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
        assert!(output.stderr.is_empty());
    }
}
#[test]
fn invalid_arguments_fail_before_terminal_or_hardware_access() {
    let exe = env!("CARGO_BIN_EXE_shr-fx");
    for arg in ["--unknown", "--data-root"] {
        let output = Command::new(exe).arg(arg).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("shr-fx:"));
    }
}
