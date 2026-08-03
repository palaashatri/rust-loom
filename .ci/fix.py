# Bounded applicator for the Motion SVG serializer and modified workspace formatting.
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "loom-motion/crates/loom-motion-app/src/main.rs"
text = path.read_text(encoding="utf-8")
start = text.index("fn xml_escape(value: &str) -> String {")
end = text.index("\nfn write_svg_frame", start)
replacement = r'''fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn export_svg_frame(doc: &CompositionDocument, time_secs: f32) -> String {
    let mut svg = String::from(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
  <rect width="1920" height="1080" fill="#101217"/>
"##,
    );
    for (index, layer) in doc.layers.iter().enumerate() {
        let sample = layer.sample(time_secs);
        let opacity = sample.opacity.clamp(0.0, 1.0);
        let scale = sample.scale.max(0.001);
        let name = xml_escape(&layer.name);
        let transform = format!(
            "translate({:.3} {:.3}) rotate({:.3}) scale({:.5})",
            sample.x, sample.y, sample.rotation, scale
        );
        match layer.layer_type.as_str() {
            "Text" => svg.push_str(&format!(
                r##"  <text transform="{transform}" opacity="{opacity:.5}" text-anchor="middle" fill="#f5f2eb" font-family="sans-serif" font-size="72">{name}</text>
"##
            )),
            "VectorShape" => svg.push_str(&format!(
                r##"  <rect transform="{transform}" opacity="{opacity:.5}" x="-180" y="-100" width="360" height="200" rx="24" fill="#b86f4b"/>
"##
            )),
            _ => svg.push_str(&format!(
                r##"  <g transform="{transform}" opacity="{opacity:.5}"><rect x="-160" y="-90" width="320" height="180" rx="16" fill="#303744" stroke="#b86f4b"/><text y="8" text-anchor="middle" fill="#f5f2eb" font-family="sans-serif" font-size="28">{name} {}</text></g>
"##,
                index + 1
            )),
        }
    }
    svg.push_str("</svg>\n");
    svg
}
'''
path.write_text(text[:start] + replacement + text[end:], encoding="utf-8", newline="\n")

subprocess.run(
    ["rustup", "toolchain", "install", "stable", "--profile", "minimal", "--component", "rustfmt"],
    cwd=root,
    check=True,
)
for workspace in ("loom-motion", "loom-encode"):
    subprocess.run(
        ["cargo", "+stable", "fmt", "--all"],
        cwd=root / workspace,
        check=True,
    )
