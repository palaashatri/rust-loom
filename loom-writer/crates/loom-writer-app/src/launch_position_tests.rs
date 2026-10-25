#[test]
fn parser_accepts_signed_physical_position_for_native_launch() {
    let parsed = super::parse_args_from(["--position", "3460,-200"]).expect("position parses");
    assert_eq!(parsed.position, Some((3460, -200)));
}

#[test]
fn parser_leaves_physical_position_unset_by_default() {
    let parsed = super::parse_args_from(std::iter::empty::<&str>()).expect("default args parse");
    assert_eq!(parsed.position, None);
}

#[test]
fn parser_rejects_missing_position_value_as_a_position_error() {
    let error = super::parse_args_from(["--position"]).unwrap_err();
    assert!(
        error.contains("--position") && !error.starts_with("unknown argument:"),
        "--position without a value should report its value error, got {error:?}"
    );
}

#[test]
fn parser_rejects_malformed_and_out_of_range_position_values() {
    for value in [
        "12",
        "x,4",
        ",4",
        "4,5,6",
        "4,5 ",
        "2147483648,0",
        "0,-2147483649",
    ] {
        let error = super::parse_args_from(["--position", value]).unwrap_err();
        assert!(
            error.contains("--position") && !error.starts_with("unknown argument:"),
            "invalid --position value {value:?} should report its value error, got {error:?}"
        );
    }
}
