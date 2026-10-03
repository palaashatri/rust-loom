use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

fn eval(formula: &str) -> Value {
    let mut sheet = Sheet::new("test");
    sheet.set_str("Z1", formula);
    evaluate(&sheet)
        .get(&CellRef::parse("Z1").unwrap())
        .cloned()
        .unwrap_or(Value::Empty)
}

#[test]
fn substitute_all_instances() {
    // SUBSTITUTE("hello world world", "world", "earth")
    assert_eq!(
        eval("=SUBSTITUTE(\"hello world world\", \"world\", \"earth\")"),
        Value::Text("hello earth earth".to_string())
    );
}

#[test]
fn substitute_specific_instance() {
    // SUBSTITUTE("hello world world", "world", "earth", 1)
    assert_eq!(
        eval("=SUBSTITUTE(\"hello world world\", \"world\", \"earth\", 1)"),
        Value::Text("hello earth world".to_string())
    );
}

#[test]
fn substitute_second_instance() {
    assert_eq!(
        eval("=SUBSTITUTE(\"hello world world\", \"world\", \"earth\", 2)"),
        Value::Text("hello world earth".to_string())
    );
}

#[test]
fn substitute_empty_old_text() {
    // Empty old_text returns text unchanged
    assert_eq!(
        eval("=SUBSTITUTE(\"hello\", \"\", \"x\")"),
        Value::Text("hello".to_string())
    );
}

#[test]
fn replace_basic() {
    // REPLACE("hello world", 1, 5, "goodbye")
    assert_eq!(
        eval("=REPLACE(\"hello world\", 1, 5, \"goodbye\")"),
        Value::Text("goodbye world".to_string())
    );
}

#[test]
fn replace_middle() {
    // REPLACE("hello world", 7, 5, "earth")
    assert_eq!(
        eval("=REPLACE(\"hello world\", 7, 5, \"earth\")"),
        Value::Text("hello earth".to_string())
    );
}

#[test]
fn replace_zero_chars() {
    // REPLACE with 0 chars: insert at position
    assert_eq!(
        eval("=REPLACE(\"hello\", 6, 0, \" world\")"),
        Value::Text("hello world".to_string())
    );
}

#[test]
fn replace_invalid_start() {
    // Start position 0 is invalid
    assert_eq!(
        eval("=REPLACE(\"hello\", 0, 1, \"x\")"),
        Value::Error(CalcError::Value)
    );
}

#[test]
fn find_basic() {
    // FIND("world", "hello world", 1)
    assert_eq!(
        eval("=FIND(\"world\", \"hello world\")"),
        Value::Number(7.0)
    );
}

#[test]
fn find_start_position() {
    // FIND with start position
    assert_eq!(eval("=FIND(\"o\", \"hello world\", 6)"), Value::Number(8.0));
}

#[test]
fn find_not_found() {
    assert_eq!(
        eval("=FIND(\"xyz\", \"hello world\")"),
        Value::Error(CalcError::Value)
    );
}

#[test]
fn find_case_sensitive() {
    // FIND is case-sensitive
    assert_eq!(
        eval("=FIND(\"World\", \"hello world\")"),
        Value::Error(CalcError::Value)
    );
}

#[test]
fn search_case_insensitive() {
    // SEARCH is case-insensitive
    assert_eq!(
        eval("=SEARCH(\"World\", \"hello world\")"),
        Value::Number(7.0)
    );
}

#[test]
fn search_with_asterisk_wildcard() {
    // SEARCH with * (any characters)
    assert_eq!(
        eval("=SEARCH(\"h*d\", \"hello world\")"),
        Value::Number(1.0)
    );
}

#[test]
fn search_with_question_mark_wildcard() {
    // SEARCH with ? (single character)
    assert_eq!(
        eval("=SEARCH(\"h?llo\", \"hello world\")"),
        Value::Number(1.0)
    );
}

#[test]
fn search_not_found() {
    assert_eq!(
        eval("=SEARCH(\"xyz\", \"hello world\")"),
        Value::Error(CalcError::Value)
    );
}

#[test]
fn proper_basic() {
    assert_eq!(
        eval("=PROPER(\"hello world\")"),
        Value::Text("Hello World".to_string())
    );
}

#[test]
fn proper_mixed_case() {
    assert_eq!(
        eval("=PROPER(\"hELLO wORLD\")"),
        Value::Text("Hello World".to_string())
    );
}

#[test]
fn proper_with_numbers() {
    assert_eq!(
        eval("=PROPER(\"hello123world\")"),
        Value::Text("Hello123World".to_string())
    );
}

#[test]
fn rept_basic() {
    assert_eq!(eval("=REPT(\"ab\", 3)"), Value::Text("ababab".to_string()));
}

#[test]
fn rept_zero_times() {
    assert_eq!(eval("=REPT(\"hello\", 0)"), Value::Text(String::new()));
}

#[test]
fn rept_negative() {
    assert_eq!(eval("=REPT(\"hello\", -1)"), Value::Error(CalcError::Value));
}

#[test]
fn exact_same_strings() {
    assert_eq!(eval("=EXACT(\"hello\", \"hello\")"), Value::Bool(true));
}

#[test]
fn exact_different_strings() {
    assert_eq!(eval("=EXACT(\"hello\", \"world\")"), Value::Bool(false));
}

#[test]
fn exact_case_sensitive() {
    assert_eq!(eval("=EXACT(\"Hello\", \"hello\")"), Value::Bool(false));
}

#[test]
fn char_basic() {
    // CHAR(65) = 'A'
    assert_eq!(eval("=CHAR(65)"), Value::Text("A".to_string()));
}

#[test]
fn char_lowercase() {
    // CHAR(97) = 'a'
    assert_eq!(eval("=CHAR(97)"), Value::Text("a".to_string()));
}

#[test]
fn char_invalid() {
    // CHAR with invalid code
    assert_eq!(eval("=CHAR(-1)"), Value::Error(CalcError::Value));
}

#[test]
fn code_basic() {
    // CODE("A") = 65
    assert_eq!(eval("=CODE(\"A\")"), Value::Number(65.0));
}

#[test]
fn code_first_character_only() {
    // CODE returns code of first character
    assert_eq!(eval("=CODE(\"hello\")"), Value::Number(104.0));
}

#[test]
fn code_empty_string() {
    // CODE on empty string
    assert_eq!(eval("=CODE(\"\")"), Value::Error(CalcError::Value));
}

#[test]
fn value_numeric_string() {
    assert_eq!(eval("=VALUE(\"123\")"), Value::Number(123.0));
}

#[test]
fn value_decimal_string() {
    assert_eq!(eval("=VALUE(\"123.45\")"), Value::Number(123.45));
}

#[test]
fn value_non_numeric() {
    assert_eq!(eval("=VALUE(\"hello\")"), Value::Error(CalcError::Value));
}

#[test]
fn value_with_spaces() {
    assert_eq!(eval("=VALUE(\"  456  \")"), Value::Number(456.0));
}

#[test]
fn text_format_zero() {
    // TEXT(123.456, "0")
    assert_eq!(
        eval("=TEXT(123.456, \"0\")"),
        Value::Text("123".to_string())
    );
}

#[test]
fn text_format_two_decimals() {
    assert_eq!(
        eval("=TEXT(123.4, \"0.00\")"),
        Value::Text("123.40".to_string())
    );
}

#[test]
fn text_format_thousands() {
    // TEXT(1234, "#,##0")
    let result = eval("=TEXT(1234, \"#,##0\")");
    // The result should be a formatted number with thousands separator
    match result {
        Value::Text(s) => {
            assert!(s.contains("1") && s.contains("234"));
        }
        _ => panic!("Expected Text, got {:?}", result),
    }
}

#[test]
fn text_format_percent() {
    // TEXT(0.5, "0%")
    assert_eq!(eval("=TEXT(0.5, \"0%\")"), Value::Text("50%".to_string()));
}

#[test]
fn text_format_percent_one_decimal() {
    assert_eq!(
        eval("=TEXT(0.123, \"0.0%\")"),
        Value::Text("12.3%".to_string())
    );
}

#[test]
fn text_format_date_iso() {
    // TEXT(45292, "yyyy-mm-dd") = 2024-01-01
    assert_eq!(
        eval("=TEXT(45292, \"yyyy-mm-dd\")"),
        Value::Text("2024-01-01".to_string())
    );
}

#[test]
fn text_format_date_ddmmyyyy() {
    // TEXT(45292, "dd/mm/yyyy") = 01/01/2024
    assert_eq!(
        eval("=TEXT(45292, \"dd/mm/yyyy\")"),
        Value::Text("01/01/2024".to_string())
    );
}

#[test]
fn text_format_date_mmm() {
    // TEXT(45292, "mmm d, yyyy") = "Jan 1, 2024"
    assert_eq!(
        eval("=TEXT(45292, \"mmm d, yyyy\")"),
        Value::Text("Jan 1, 2024".to_string())
    );
}

#[test]
fn clean_basic() {
    // CLEAN removes non-printable characters
    let result = eval("=CLEAN(\"hello world\")");
    assert_eq!(result, Value::Text("hello world".to_string()));
}

#[test]
fn substitute_with_number_coercion() {
    // Numbers should be coerced to text
    assert_eq!(
        eval("=SUBSTITUTE(123, \"2\", \"X\")"),
        Value::Text("1X3".to_string())
    );
}

#[test]
fn find_from_start() {
    // FIND("hello", "hello world", 1)
    assert_eq!(
        eval("=FIND(\"hello\", \"hello world\", 1)"),
        Value::Number(1.0)
    );
}

#[test]
fn proper_single_word() {
    assert_eq!(eval("=PROPER(\"hello\")"), Value::Text("Hello".to_string()));
}

#[test]
fn rept_single_repeat() {
    assert_eq!(eval("=REPT(\"x\", 1)"), Value::Text("x".to_string()));
}

#[test]
fn exact_empty_strings() {
    assert_eq!(eval("=EXACT(\"\", \"\")"), Value::Bool(true));
}

#[test]
fn char_space() {
    // CHAR(32) = space
    assert_eq!(eval("=CHAR(32)"), Value::Text(" ".to_string()));
}

#[test]
fn code_digit() {
    // CODE("0") = 48
    assert_eq!(eval("=CODE(\"0\")"), Value::Number(48.0));
}

#[test]
fn value_negative() {
    assert_eq!(eval("=VALUE(\"-123\")"), Value::Number(-123.0));
}

#[test]
fn text_default_format() {
    // Unknown format should display number as-is
    let result = eval("=TEXT(123.456, \"unknown\")");
    match result {
        Value::Text(_) => {
            // Just verify it's text
        }
        _ => panic!("Expected Text"),
    }
}

#[test]
fn search_wildcard_escaped() {
    // Escaped * should match literal *
    assert_eq!(eval("=SEARCH(\"~*\", \"a*b\")"), Value::Number(2.0));
}

#[test]
fn replace_beyond_text_length() {
    // REPLACE with end beyond text length
    assert_eq!(
        eval("=REPLACE(\"hello\", 4, 10, \"X\")"),
        Value::Text("helX".to_string())
    );
}

#[test]
fn find_empty_search_text() {
    // Excel: empty find_text matches at start_num.
    assert_eq!(eval("=FIND(\"\", \"hello\")"), Value::Number(1.0));
}

#[test]
fn substitute_instance_out_of_range() {
    // Instance beyond occurrences just returns original
    let result = eval("=SUBSTITUTE(\"hello\", \"l\", \"X\", 10)");
    match result {
        Value::Text(s) => {
            // When instance is beyond the count, only the rest is returned
            // "hello" has 2 l's, asking for 10th instance returns the whole string
            assert_eq!(s, "hello");
        }
        _ => panic!("Expected Text"),
    }
}

#[test]
fn text_boolean_value() {
    // Excel does not format booleans: TEXT(TRUE,"0") is "TRUE".
    assert_eq!(eval("=TEXT(TRUE, \"0\")"), Value::Text("TRUE".to_string()));
}

#[test]
fn search_start_position() {
    // SEARCH with start position
    assert_eq!(
        eval("=SEARCH(\"o\", \"hello world\", 6)"),
        Value::Number(8.0)
    );
}
