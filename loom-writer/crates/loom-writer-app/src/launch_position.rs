pub(super) fn parse(value: &str) -> Result<(i32, i32), String> {
    let (x, y) = value
        .split_once(',')
        .ok_or_else(|| "--position must be X,Y with signed 32-bit coordinates".to_string())?;
    if y.contains(',') {
        return Err("--position must be X,Y with signed 32-bit coordinates".to_string());
    }
    let parse = |coordinate: &str| {
        coordinate
            .parse::<i32>()
            .map_err(|_| "--position must be X,Y with signed 32-bit coordinates".to_string())
    };
    Ok((parse(x)?, parse(y)?))
}
