//! Command-line options shared by the app, headless renderer, and journeys.

pub(super) struct Args {
    pub(super) screenshot: Option<String>,
    pub(super) smoke: bool,
    pub(super) example: bool,
    pub(super) palette: bool,
    pub(super) chart: bool,
    pub(super) objects: bool,
    pub(super) inspector: bool,
    pub(super) journey: Option<String>,
    pub(super) size: (u32, u32),
    pub(super) theme: String,
    pub(super) rtl: bool,
    pub(super) open: Option<String>,
    pub(super) template_chooser: bool,
    pub(super) text_scale: f32,
    pub(super) zoom: Option<f32>,
}

pub(super) fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

pub(super) fn parse_args_from<I, S>(raw_args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = Args {
        screenshot: None,
        smoke: false,
        example: false,
        palette: false,
        chart: false,
        objects: false,
        inspector: false,
        journey: None,
        size: super::DEFAULT_SIZE,
        theme: "light".to_string(),
        rtl: false,
        open: None,
        template_chooser: false,
        text_scale: 1.0,
        zoom: None,
    };
    let mut it = raw_args.into_iter().map(Into::into);
    while let Some(argument) = it.next() {
        match argument.as_str() {
            "--screenshot" => args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?),
            "--smoke" => args.smoke = true,
            "--example" => args.example = true,
            "--palette" => args.palette = true,
            "--chart" => args.chart = true,
            "--objects" => args.objects = true,
            "--inspector" => args.inspector = true,
            "--journey" => {
                args.journey = Some(it.next().ok_or("--journey needs an output directory")?)
            }
            "--size" => {
                let value = it.next().ok_or("--size needs WxH")?;
                let (width, height) = value.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    width.parse().map_err(|_| "bad --size width")?,
                    height.parse().map_err(|_| "bad --size height")?,
                );
            }
            "--theme" => {
                let theme = it.next().ok_or("--theme needs a name")?;
                if !matches!(theme.as_str(), "light" | "dark" | "high-contrast") {
                    return Err(format!("unknown theme: {theme}"));
                }
                args.theme = theme;
            }
            "--rtl" => args.rtl = true,
            "--template-chooser" => args.template_chooser = true,
            "--text-scale" => {
                let scale: f32 = it
                    .next()
                    .ok_or("--text-scale needs a factor")?
                    .parse()
                    .map_err(|_| "bad --text-scale factor")?;
                if !(1.0..=2.0).contains(&scale) {
                    return Err("--text-scale must be between 1.0 and 2.0".to_string());
                }
                args.text_scale = scale;
            }
            "--zoom" => {
                let zoom: f32 = it
                    .next()
                    .ok_or("--zoom needs a factor")?
                    .parse()
                    .map_err(|_| "bad --zoom factor")?;
                if !(0.5..=3.0).contains(&zoom) {
                    return Err("--zoom must be between 0.5 and 3.0".to_string());
                }
                args.zoom = Some(zoom);
            }
            "--open" => args.open = Some(it.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string())
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}
