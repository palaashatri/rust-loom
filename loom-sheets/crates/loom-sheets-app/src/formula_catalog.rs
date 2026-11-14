//! Function catalog, parameter signatures, and summaries for formula autocomplete.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FunctionDoc {
    pub name: &'static str,
    pub params: &'static [&'static str],
    pub summary: &'static str,
}

pub(crate) fn format_signature(doc: &FunctionDoc) -> String {
    format!("{}({})", doc.name, doc.params.join(", "))
}

pub(crate) fn split_signature_for_arg(
    doc: &FunctionDoc,
    arg_index: usize,
) -> (String, String, String) {
    if doc.params.is_empty() {
        return (format!("{}(", doc.name), String::new(), ")".to_string());
    }
    let idx = arg_index.min(doc.params.len() - 1);
    let mut prefix = format!("{}(", doc.name);
    for (i, param) in doc.params.iter().take(idx).enumerate() {
        if i > 0 {
            prefix.push_str(", ");
        }
        prefix.push_str(param);
    }
    if idx > 0 {
        prefix.push_str(", ");
    }
    let active = doc.params[idx].to_string();
    let mut suffix = String::new();
    for param in doc.params.iter().skip(idx + 1) {
        suffix.push_str(", ");
        suffix.push_str(param);
    }
    suffix.push(')');
    (prefix, active, suffix)
}

pub(crate) fn lookup_function(name: &str) -> Option<&'static FunctionDoc> {
    let upper = name.to_ascii_uppercase();
    FUNCTIONS.iter().find(|f| f.name == upper)
}

pub(crate) fn matching_functions(prefix: &str, limit: usize) -> Vec<&'static FunctionDoc> {
    if prefix.is_empty() || limit == 0 {
        return Vec::new();
    }
    let upper = prefix.to_ascii_uppercase();
    let mut exact_prefix: Vec<&'static FunctionDoc> = FUNCTIONS
        .iter()
        .filter(|f| f.name.starts_with(&upper))
        .collect();
    exact_prefix.truncate(limit);
    exact_prefix
}

const FUNCTIONS: &[FunctionDoc] = &[
    FunctionDoc {
        name: "ABS",
        params: &["number"],
        summary: "Absolute value of a number.",
    },
    FunctionDoc {
        name: "AND",
        params: &["logical1", "[logical2]", "..."],
        summary: "TRUE if all arguments are TRUE.",
    },
    FunctionDoc {
        name: "AVERAGE",
        params: &["number1", "[number2]", "..."],
        summary: "Arithmetic mean of numeric arguments.",
    },
    FunctionDoc {
        name: "AVERAGEIF",
        params: &["range", "criteria", "[average_range]"],
        summary: "Average of cells matching a condition.",
    },
    FunctionDoc {
        name: "AVERAGEIFS",
        params: &["average_range", "criteria_range1", "criteria1", "..."],
        summary: "Average of cells matching multiple criteria.",
    },
    FunctionDoc {
        name: "CEILING",
        params: &["number", "significance"],
        summary: "Rounds a number up to the nearest multiple.",
    },
    FunctionDoc {
        name: "CHAR",
        params: &["number"],
        summary: "Character specified by a code number.",
    },
    FunctionDoc {
        name: "CHOOSE",
        params: &["index_num", "value1", "[value2]", "..."],
        summary: "Selects a value from a list by index.",
    },
    FunctionDoc {
        name: "CHOOSECOLS",
        params: &["array", "col_num1", "[col_num2]", "..."],
        summary: "Returns specified columns from an array.",
    },
    FunctionDoc {
        name: "CHOOSEROWS",
        params: &["array", "row_num1", "[row_num2]", "..."],
        summary: "Returns specified rows from an array.",
    },
    FunctionDoc {
        name: "CLEAN",
        params: &["text"],
        summary: "Removes non-printable characters from text.",
    },
    FunctionDoc {
        name: "CODE",
        params: &["text"],
        summary: "Numeric code for the first character.",
    },
    FunctionDoc {
        name: "COLUMNS",
        params: &["array"],
        summary: "Number of columns in a reference.",
    },
    FunctionDoc {
        name: "CONCATENATE",
        params: &["text1", "[text2]", "..."],
        summary: "Joins several text strings into one.",
    },
    FunctionDoc {
        name: "CORREL",
        params: &["array1", "array2"],
        summary: "Correlation coefficient between two ranges.",
    },
    FunctionDoc {
        name: "COUNT",
        params: &["value1", "[value2]", "..."],
        summary: "Counts cells that contain numbers.",
    },
    FunctionDoc {
        name: "COUNTA",
        params: &["value1", "[value2]", "..."],
        summary: "Counts non-empty cells in a range.",
    },
    FunctionDoc {
        name: "COUNTBLANK",
        params: &["range"],
        summary: "Counts empty cells in a range.",
    },
    FunctionDoc {
        name: "COUNTIF",
        params: &["range", "criteria"],
        summary: "Counts cells matching a condition.",
    },
    FunctionDoc {
        name: "COUNTIFS",
        params: &["criteria_range1", "criteria1", "..."],
        summary: "Counts cells matching multiple criteria.",
    },
    FunctionDoc {
        name: "DATE",
        params: &["year", "month", "day"],
        summary: "Creates a date from year, month, and day.",
    },
    FunctionDoc {
        name: "DATEDIF",
        params: &["start_date", "end_date", "unit"],
        summary: "Difference between two dates in units.",
    },
    FunctionDoc {
        name: "DATEVALUE",
        params: &["date_text"],
        summary: "Converts a date string to a serial number.",
    },
    FunctionDoc {
        name: "DAY",
        params: &["serial_number"],
        summary: "Day of the month (1 to 31).",
    },
    FunctionDoc {
        name: "DAYS",
        params: &["end_date", "start_date"],
        summary: "Number of days between two dates.",
    },
    FunctionDoc {
        name: "DB",
        params: &["cost", "salvage", "life", "period", "[month]"],
        summary: "Fixed-declining balance depreciation.",
    },
    FunctionDoc {
        name: "DDB",
        params: &["cost", "salvage", "life", "period", "[factor]"],
        summary: "Double-declining balance depreciation.",
    },
    FunctionDoc {
        name: "DROP",
        params: &["array", "rows", "[columns]"],
        summary: "Drops rows or columns from an array.",
    },
    FunctionDoc {
        name: "EDATE",
        params: &["start_date", "months"],
        summary: "Date offset by a number of months.",
    },
    FunctionDoc {
        name: "EOMONTH",
        params: &["start_date", "months"],
        summary: "Last day of the month offset by months.",
    },
    FunctionDoc {
        name: "EXACT",
        params: &["text1", "text2"],
        summary: "Checks whether two text strings are identical.",
    },
    FunctionDoc {
        name: "EXP",
        params: &["number"],
        summary: "Returns e raised to the power of number.",
    },
    FunctionDoc {
        name: "EXPAND",
        params: &["array", "rows", "[columns]", "[pad_with]"],
        summary: "Expands an array to specified dimensions.",
    },
    FunctionDoc {
        name: "FALSE",
        params: &[],
        summary: "Returns the logical value FALSE.",
    },
    FunctionDoc {
        name: "FILTER",
        params: &["array", "include", "[if_empty]"],
        summary: "Filters a range based on criteria.",
    },
    FunctionDoc {
        name: "FIND",
        params: &["find_text", "within_text", "[start_num]"],
        summary: "Case-sensitive position of text.",
    },
    FunctionDoc {
        name: "FORECAST.LINEAR",
        params: &["x", "known_y_s", "known_x_s"],
        summary: "Predicts a value using linear regression.",
    },
    FunctionDoc {
        name: "FV",
        params: &["rate", "nper", "pmt", "[pv]", "[type]"],
        summary: "Future value of an investment.",
    },
    FunctionDoc {
        name: "HLOOKUP",
        params: &[
            "lookup_value",
            "table_array",
            "row_index_num",
            "[range_lookup]",
        ],
        summary: "Looks up a value in the top row.",
    },
    FunctionDoc {
        name: "HOUR",
        params: &["serial_number"],
        summary: "Hour of a time value (0 to 23).",
    },
    FunctionDoc {
        name: "HSTACK",
        params: &["array1", "[array2]", "..."],
        summary: "Appends arrays horizontally.",
    },
    FunctionDoc {
        name: "IF",
        params: &["logical_test", "value_if_true", "[value_if_false]"],
        summary: "Returns one value if TRUE, another if FALSE.",
    },
    FunctionDoc {
        name: "IFERROR",
        params: &["value", "value_if_error"],
        summary: "Returns fallback value if formula errors.",
    },
    FunctionDoc {
        name: "IFNA",
        params: &["value", "value_if_na"],
        summary: "Returns fallback value if result is #N/A.",
    },
    FunctionDoc {
        name: "IFS",
        params: &["logical_test1", "value_if_true1", "..."],
        summary: "Checks multiple conditions in order.",
    },
    FunctionDoc {
        name: "INDEX",
        params: &["array", "row_num", "[column_num]"],
        summary: "Value at a row and column in a range.",
    },
    FunctionDoc {
        name: "INT",
        params: &["number"],
        summary: "Rounds a number down to nearest integer.",
    },
    FunctionDoc {
        name: "INTERCEPT",
        params: &["known_y_s", "known_x_s"],
        summary: "Y-intercept of a linear regression line.",
    },
    FunctionDoc {
        name: "IPMT",
        params: &["rate", "per", "nper", "pv", "[fv]", "[type]"],
        summary: "Interest payment for a period.",
    },
    FunctionDoc {
        name: "IRR",
        params: &["values", "[guess]"],
        summary: "Internal rate of return for cash flows.",
    },
    FunctionDoc {
        name: "ISBLANK",
        params: &["value"],
        summary: "TRUE if the referenced cell is empty.",
    },
    FunctionDoc {
        name: "ISERROR",
        params: &["value"],
        summary: "TRUE if the value is any error.",
    },
    FunctionDoc {
        name: "ISLOGICAL",
        params: &["value"],
        summary: "TRUE if the value is a boolean.",
    },
    FunctionDoc {
        name: "ISNA",
        params: &["value"],
        summary: "TRUE if the value is #N/A.",
    },
    FunctionDoc {
        name: "ISNUMBER",
        params: &["value"],
        summary: "TRUE if the value is a number.",
    },
    FunctionDoc {
        name: "ISOWEEKNUM",
        params: &["date"],
        summary: "ISO week number of the year for a date.",
    },
    FunctionDoc {
        name: "ISTEXT",
        params: &["value"],
        summary: "TRUE if the value is text.",
    },
    FunctionDoc {
        name: "LARGE",
        params: &["array", "k"],
        summary: "Returns the k-th largest value.",
    },
    FunctionDoc {
        name: "LEFT",
        params: &["text", "[num_chars]"],
        summary: "First characters from a text string.",
    },
    FunctionDoc {
        name: "LEN",
        params: &["text"],
        summary: "Number of characters in a text string.",
    },
    FunctionDoc {
        name: "LET",
        params: &["name1", "name_value1", "calculation"],
        summary: "Assigns names to calculation results.",
    },
    FunctionDoc {
        name: "LN",
        params: &["number"],
        summary: "Natural logarithm of a number.",
    },
    FunctionDoc {
        name: "LOG",
        params: &["number", "[base]"],
        summary: "Logarithm of a number to a specified base.",
    },
    FunctionDoc {
        name: "LOG10",
        params: &["number"],
        summary: "Base-10 logarithm of a number.",
    },
    FunctionDoc {
        name: "LOWER",
        params: &["text"],
        summary: "Converts text to lowercase.",
    },
    FunctionDoc {
        name: "MATCH",
        params: &["lookup_value", "lookup_array", "[match_type]"],
        summary: "Relative position of an item in a range.",
    },
    FunctionDoc {
        name: "MAX",
        params: &["number1", "[number2]", "..."],
        summary: "Largest value in a set of values.",
    },
    FunctionDoc {
        name: "MAXIFS",
        params: &["max_range", "criteria_range1", "criteria1", "..."],
        summary: "Maximum value matching criteria.",
    },
    FunctionDoc {
        name: "MEDIAN",
        params: &["number1", "[number2]", "..."],
        summary: "Median of the given numbers.",
    },
    FunctionDoc {
        name: "MID",
        params: &["text", "start_num", "num_chars"],
        summary: "Characters from the middle of a text string.",
    },
    FunctionDoc {
        name: "MIN",
        params: &["number1", "[number2]", "..."],
        summary: "Smallest value in a set of values.",
    },
    FunctionDoc {
        name: "MINIFS",
        params: &["min_range", "criteria_range1", "criteria1", "..."],
        summary: "Minimum value matching criteria.",
    },
    FunctionDoc {
        name: "MINUTE",
        params: &["serial_number"],
        summary: "Minute of a time value (0 to 59).",
    },
    FunctionDoc {
        name: "MOD",
        params: &["number", "divisor"],
        summary: "Remainder after division.",
    },
    FunctionDoc {
        name: "MODE.SNGL",
        params: &["number1", "[number2]", "..."],
        summary: "Most frequently occurring value.",
    },
    FunctionDoc {
        name: "MONTH",
        params: &["serial_number"],
        summary: "Month of a date (1 to 12).",
    },
    FunctionDoc {
        name: "MROUND",
        params: &["number", "multiple"],
        summary: "Rounds to the nearest multiple.",
    },
    FunctionDoc {
        name: "NETWORKDAYS",
        params: &["start_date", "end_date", "[holidays]"],
        summary: "Whole workdays between two dates.",
    },
    FunctionDoc {
        name: "NOW",
        params: &[],
        summary: "Current date and time.",
    },
    FunctionDoc {
        name: "NPER",
        params: &["rate", "pmt", "pv", "[fv]", "[type]"],
        summary: "Number of periods for an investment.",
    },
    FunctionDoc {
        name: "NPV",
        params: &["rate", "value1", "[value2]", "..."],
        summary: "Net present value of an investment.",
    },
    FunctionDoc {
        name: "OR",
        params: &["logical1", "[logical2]", "..."],
        summary: "TRUE if any argument is TRUE.",
    },
    FunctionDoc {
        name: "PERCENTILE.EXC",
        params: &["array", "k"],
        summary: "k-th percentile (exclusive 0..1).",
    },
    FunctionDoc {
        name: "PERCENTILE.INC",
        params: &["array", "k"],
        summary: "k-th percentile (inclusive 0..1).",
    },
    FunctionDoc {
        name: "PI",
        params: &[],
        summary: "Mathematical constant pi (3.14159...).",
    },
    FunctionDoc {
        name: "PMT",
        params: &["rate", "nper", "pv", "[fv]", "[type]"],
        summary: "Periodic payment for a loan.",
    },
    FunctionDoc {
        name: "POWER",
        params: &["number", "power"],
        summary: "Result of a number raised to a power.",
    },
    FunctionDoc {
        name: "PPMT",
        params: &["rate", "per", "nper", "pv", "[fv]", "[type]"],
        summary: "Principal payment for a period.",
    },
    FunctionDoc {
        name: "PRODUCT",
        params: &["number1", "[number2]", "..."],
        summary: "Multiplies all numbers given as arguments.",
    },
    FunctionDoc {
        name: "PROPER",
        params: &["text"],
        summary: "Capitalizes the first letter of each word.",
    },
    FunctionDoc {
        name: "PV",
        params: &["rate", "nper", "pmt", "[fv]", "[type]"],
        summary: "Present value of an investment.",
    },
    FunctionDoc {
        name: "QUARTILE.EXC",
        params: &["array", "quart"],
        summary: "Quartile of a data set (exclusive).",
    },
    FunctionDoc {
        name: "QUARTILE.INC",
        params: &["array", "quart"],
        summary: "Quartile of a data set (inclusive).",
    },
    FunctionDoc {
        name: "QUOTIENT",
        params: &["numerator", "denominator"],
        summary: "Integer portion of a division.",
    },
    FunctionDoc {
        name: "RAND",
        params: &[],
        summary: "Random number between 0 and 1.",
    },
    FunctionDoc {
        name: "RANDARRAY",
        params: &["[rows]", "[columns]", "[min]", "[max]", "[whole]"],
        summary: "Array of random numbers.",
    },
    FunctionDoc {
        name: "RANDBETWEEN",
        params: &["bottom", "top"],
        summary: "Random integer between two numbers.",
    },
    FunctionDoc {
        name: "RANK.AVG",
        params: &["number", "ref", "[order]"],
        summary: "Rank of a number (averages ties).",
    },
    FunctionDoc {
        name: "RANK.EQ",
        params: &["number", "ref", "[order]"],
        summary: "Rank of a number in a list.",
    },
    FunctionDoc {
        name: "RATE",
        params: &["nper", "pmt", "pv", "[fv]", "[type]", "[guess]"],
        summary: "Interest rate per period of an annuity.",
    },
    FunctionDoc {
        name: "REPLACE",
        params: &["old_text", "start_num", "num_chars", "new_text"],
        summary: "Replaces part of a text string.",
    },
    FunctionDoc {
        name: "REPT",
        params: &["text", "number_times"],
        summary: "Repeats text a given number of times.",
    },
    FunctionDoc {
        name: "RIGHT",
        params: &["text", "[num_chars]"],
        summary: "Last characters from a text string.",
    },
    FunctionDoc {
        name: "ROUND",
        params: &["number", "num_digits"],
        summary: "Rounds a number to specified digits.",
    },
    FunctionDoc {
        name: "ROUNDDOWN",
        params: &["number", "num_digits"],
        summary: "Rounds a number down toward zero.",
    },
    FunctionDoc {
        name: "ROUNDUP",
        params: &["number", "num_digits"],
        summary: "Rounds a number up away from zero.",
    },
    FunctionDoc {
        name: "ROWS",
        params: &["array"],
        summary: "Number of rows in a reference.",
    },
    FunctionDoc {
        name: "SEARCH",
        params: &["find_text", "within_text", "[start_num]"],
        summary: "Case-insensitive position of text.",
    },
    FunctionDoc {
        name: "SECOND",
        params: &["serial_number"],
        summary: "Seconds of a time value (0 to 59).",
    },
    FunctionDoc {
        name: "SEQUENCE",
        params: &["rows", "[columns]", "[start]", "[step]"],
        summary: "Generates a sequence of numbers.",
    },
    FunctionDoc {
        name: "SIGN",
        params: &["number"],
        summary: "Sign of a number (1, 0, or -1).",
    },
    FunctionDoc {
        name: "SLN",
        params: &["cost", "salvage", "life"],
        summary: "Straight-line depreciation for one period.",
    },
    FunctionDoc {
        name: "SLOPE",
        params: &["known_y_s", "known_x_s"],
        summary: "Slope of a linear regression line.",
    },
    FunctionDoc {
        name: "SMALL",
        params: &["array", "k"],
        summary: "Returns the k-th smallest value.",
    },
    FunctionDoc {
        name: "SORT",
        params: &["array", "[sort_index]", "[sort_order]", "[by_col]"],
        summary: "Sorts the contents of a range.",
    },
    FunctionDoc {
        name: "SORTBY",
        params: &["array", "by_array1", "[sort_order1]", "..."],
        summary: "Sorts a range by another range.",
    },
    FunctionDoc {
        name: "SQRT",
        params: &["number"],
        summary: "Positive square root of a number.",
    },
    FunctionDoc {
        name: "STDEV.P",
        params: &["number1", "[number2]", "..."],
        summary: "Standard deviation of a population.",
    },
    FunctionDoc {
        name: "STDEV.S",
        params: &["number1", "[number2]", "..."],
        summary: "Standard deviation of a sample.",
    },
    FunctionDoc {
        name: "SUBSTITUTE",
        params: &["text", "old_text", "new_text", "[instance_num]"],
        summary: "Replaces occurrences of text.",
    },
    FunctionDoc {
        name: "SUM",
        params: &["number1", "[number2]", "..."],
        summary: "Adds all numbers in a range of cells.",
    },
    FunctionDoc {
        name: "SUMIF",
        params: &["range", "criteria", "[sum_range]"],
        summary: "Sums cells matching a condition.",
    },
    FunctionDoc {
        name: "SUMIFS",
        params: &["sum_range", "criteria_range1", "criteria1", "..."],
        summary: "Sums cells matching multiple criteria.",
    },
    FunctionDoc {
        name: "SUMPRODUCT",
        params: &["array1", "[array2]", "..."],
        summary: "Sum of products of corresponding arrays.",
    },
    FunctionDoc {
        name: "SWITCH",
        params: &["expression", "value1", "result1", "..."],
        summary: "Matches an expression against values.",
    },
    FunctionDoc {
        name: "SYD",
        params: &["cost", "salvage", "life", "per"],
        summary: "Sum-of-years' digits depreciation.",
    },
    FunctionDoc {
        name: "TAKE",
        params: &["array", "rows", "[columns]"],
        summary: "Takes rows or columns from an array.",
    },
    FunctionDoc {
        name: "TEXT",
        params: &["value", "format_text"],
        summary: "Formats a number as text.",
    },
    FunctionDoc {
        name: "TEXTAFTER",
        params: &["text", "delimiter", "[instance_num]"],
        summary: "Text occurring after a delimiter.",
    },
    FunctionDoc {
        name: "TEXTBEFORE",
        params: &["text", "delimiter", "[instance_num]"],
        summary: "Text occurring before a delimiter.",
    },
    FunctionDoc {
        name: "TEXTJOIN",
        params: &["delimiter", "ignore_empty", "text1", "..."],
        summary: "Joins text with a delimiter.",
    },
    FunctionDoc {
        name: "TEXTSPLIT",
        params: &["text", "col_delimiter", "[row_delimiter]"],
        summary: "Splits text into cells by delimiter.",
    },
    FunctionDoc {
        name: "TIME",
        params: &["hour", "minute", "second"],
        summary: "Creates a time serial number.",
    },
    FunctionDoc {
        name: "TOCOL",
        params: &["array", "[ignore]", "[scan_by_column]"],
        summary: "Transforms an array into a single column.",
    },
    FunctionDoc {
        name: "TODAY",
        params: &[],
        summary: "Current date.",
    },
    FunctionDoc {
        name: "TOROW",
        params: &["array", "[ignore]", "[scan_by_column]"],
        summary: "Transforms an array into a single row.",
    },
    FunctionDoc {
        name: "TRANSPOSE",
        params: &["array"],
        summary: "Flips rows and columns of an array.",
    },
    FunctionDoc {
        name: "TRIM",
        params: &["text"],
        summary: "Removes extra spaces from text.",
    },
    FunctionDoc {
        name: "TRUE",
        params: &[],
        summary: "Returns the logical value TRUE.",
    },
    FunctionDoc {
        name: "TRUNC",
        params: &["number", "[num_digits]"],
        summary: "Truncates a number to an integer.",
    },
    FunctionDoc {
        name: "UNIQUE",
        params: &["array", "[by_col]", "[exactly_once]"],
        summary: "Unique values from a range.",
    },
    FunctionDoc {
        name: "UPPER",
        params: &["text"],
        summary: "Converts text to uppercase.",
    },
    FunctionDoc {
        name: "VALUE",
        params: &["text"],
        summary: "Converts a text string to a number.",
    },
    FunctionDoc {
        name: "VAR.P",
        params: &["number1", "[number2]", "..."],
        summary: "Variance of a population.",
    },
    FunctionDoc {
        name: "VAR.S",
        params: &["number1", "[number2]", "..."],
        summary: "Variance of a sample.",
    },
    FunctionDoc {
        name: "VLOOKUP",
        params: &[
            "lookup_value",
            "table_array",
            "col_index_num",
            "[range_lookup]",
        ],
        summary: "Looks up a value in the first column.",
    },
    FunctionDoc {
        name: "VSTACK",
        params: &["array1", "[array2]", "..."],
        summary: "Appends arrays vertically.",
    },
    FunctionDoc {
        name: "WEEKDAY",
        params: &["serial_number", "[return_type]"],
        summary: "Day of the week as a number.",
    },
    FunctionDoc {
        name: "WEEKNUM",
        params: &["serial_number", "[return_type]"],
        summary: "Week number of the year.",
    },
    FunctionDoc {
        name: "WORKDAY",
        params: &["start_date", "days", "[holidays]"],
        summary: "Date a number of workdays away.",
    },
    FunctionDoc {
        name: "XIRR",
        params: &["values", "dates", "[guess]"],
        summary: "Internal rate of return for irregular dates.",
    },
    FunctionDoc {
        name: "XLOOKUP",
        params: &[
            "lookup_value",
            "lookup_array",
            "return_array",
            "[if_not_found]",
        ],
        summary: "Searches a range and returns a match.",
    },
    FunctionDoc {
        name: "XMATCH",
        params: &["lookup_value", "lookup_array", "[match_mode]"],
        summary: "Position of an item in a range.",
    },
    FunctionDoc {
        name: "XNPV",
        params: &["rate", "values", "dates"],
        summary: "Net present value for irregular dates.",
    },
    FunctionDoc {
        name: "XOR",
        params: &["logical1", "[logical2]", "..."],
        summary: "Logical exclusive OR of all arguments.",
    },
    FunctionDoc {
        name: "YEAR",
        params: &["serial_number"],
        summary: "Year of a date.",
    },
    FunctionDoc {
        name: "YEARFRAC",
        params: &["start_date", "end_date", "[basis]"],
        summary: "Fraction of the year between two dates.",
    },
];
