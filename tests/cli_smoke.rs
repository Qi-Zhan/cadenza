use std::process::Command;

use cadenza::cli::{CommandKind, ParsedArgs, parse_args};

#[test]
fn shows_help_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_cadenza"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cadenza"));
    assert!(!stdout.contains("fetch-classics"));
    assert!(stdout.contains("cadenza [path]"));
    assert!(!stdout.contains("--library"));
}

#[test]
fn parses_positional_library_argument_for_player() {
    let args = vec!["cadenza".to_string(), "/tmp/cadenza-music".to_string()];

    let parsed = parse_args(args);

    assert_eq!(
        parsed,
        ParsedArgs::Run(CommandKind::RunUi {
            library_root: "/tmp/cadenza-music".into(),
        })
    );
}

#[test]
fn defaults_to_current_directory_when_no_path_is_provided() {
    let parsed = parse_args(vec!["cadenza".to_string()]);

    assert_eq!(
        parsed,
        ParsedArgs::Run(CommandKind::RunUi {
            library_root: std::env::current_dir().unwrap(),
        })
    );
}
