from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


# Present: only the real slide browser is exposed. Layout/media/chart modes are
# not persisted workspaces yet, and decorative sample metrics are not slide data.
present_path = ROOT / "loom-present/crates/loom-present-app/ui/app.slint"
present = present_path.read_text()
present = present.replace("PaneTabs, RailButton, Slider,", "PaneTabs, Slider,")
present = replace_once(
    present,
    '''            Rectangle {
                width: 66px;
                background: Theme.palette().surface;
                VerticalLayout {
                    spacing: 6px;
                    padding-top: 10px;
                    RailButton { text: "Slides"; icon: "slide"; checked: true; }
                    RailButton { text: "Layouts"; icon: "grid"; }
                    RailButton { text: "Elements"; icon: "plus-circle"; }
                    RailButton { text: "Media"; icon: "image"; }
                    RailButton { text: "Charts"; icon: "table"; }
                }
            }
            Rectangle { width: 1px; background: Theme.palette().border; }
''',
    "",
    "Present inactive mode rail",
)
present = replace_once(
    present,
    '''                            HorizontalLayout {
                                spacing: 12px;
                                Rectangle { width: 30%; height: 66px; border-radius: 8px; background: Theme.palette().content-accent-soft; Text { text: "42%"; color: Theme.palette().content-accent; font-size: 25px; font-weight: 700; horizontal-alignment: center; vertical-alignment: center; } }
                                Rectangle { width: 30%; height: 66px; border-radius: 8px; background: Theme.palette().content-cool-soft; Text { text: "Local"; color: Theme.palette().content-cool; font-size: 18px; font-weight: 650; horizontal-alignment: center; vertical-alignment: center; } }
                                Rectangle { horizontal-stretch: 1; height: 66px; border-radius: 8px; background: Theme.palette().content-accent-soft; Text { text: "Loom"; color: Theme.palette().paper-ink; font-size: 18px; font-weight: 650; horizontal-alignment: center; vertical-alignment: center; } }
                            }
''',
    "",
    "Present decorative sample metrics",
)
present_path.write_text(present.rstrip() + "\n")

# Photo: the current engine provides layers, compositing and adjustments, not
# brush/heal/crop/text interaction surfaces. Keep only implemented workflows.
photo_path = ROOT / "loom-photo/crates/loom-photo-app/ui/app.slint"
photo = photo_path.read_text()
photo = photo.replace("    RailButton, Slider,", "    Slider,")
photo = replace_once(
    photo,
    '''            Rectangle {
                width: 66px;
                background: Theme.palette().surface;
                border-width: 0px;
                VerticalLayout {
                    spacing: 6px;
                    padding-top: 10px;
                    alignment: start;
                    RailButton { text: "Move"; icon: "cursor"; checked: root.active-tool == 0; clicked => { root.active-tool = 0; root.select-tool("move"); } }
                    RailButton { text: "Select"; icon: "scale"; checked: root.active-tool == 1; clicked => { root.active-tool = 1; root.select-tool("select"); } }
                    RailButton { text: "Brush"; icon: "brush"; checked: root.active-tool == 2; clicked => { root.active-tool = 2; root.select-tool("brush"); } }
                    RailButton { text: "Heal"; icon: "wand"; checked: root.active-tool == 3; clicked => { root.active-tool = 3; root.select-tool("heal"); } }
                    RailButton { text: "Crop"; icon: "scale"; checked: root.active-tool == 4; clicked => { root.active-tool = 4; root.select-tool("crop"); } }
                    RailButton { text: "Text"; icon: "text"; checked: root.active-tool == 5; clicked => { root.active-tool = 5; root.select-tool("text"); } }
                }
            }
            Rectangle { width: 1px; background: Theme.palette().border; }

''',
    "",
    "Photo unimplemented tool rail",
)
photo_path.write_text(photo.rstrip() + "\n")

# Video: retain direct timeline operations and media browser. Unimplemented
# Titles/Effects/Color workspaces and pseudo Select/Blade modes are hidden.
video_path = ROOT / "loom-video/crates/loom-video-app/ui/app.slint"
video = video_path.read_text()
video = video.replace("PaneTabs, RailButton, Slider,", "PaneTabs, Slider,")
video = replace_once(
    video,
    '''            ToolButton { icon: "cursor"; text: "Select"; checked: root.active-nle-tool == "Select"; clicked => { root.active-nle-tool = "Select"; root.select-nle-tool("Select"); } }
            ToolButton { icon: "scissors"; text: "Blade"; checked: root.active-nle-tool == "Blade"; clicked => { root.active-nle-tool = "Blade"; root.select-nle-tool("Blade"); } }
            ToolButton { icon: "timeline"; text: "Snap"; checked: root.snap-enabled; clicked => { root.snap-enabled = !root.snap-enabled; root.toggle-snap(); } }
            Rectangle { horizontal-stretch: 1; background: transparent; }
''',
    '''            Rectangle { horizontal-stretch: 1; background: transparent; }
''',
    "Video pseudo edit modes",
)
video = replace_once(
    video,
    '''            Rectangle {
                width: 66px;
                background: Theme.palette().surface;
                VerticalLayout {
                    spacing: 6px; padding-top: 10px;
                    RailButton { text: "Media"; icon: "video"; checked: true; }
                    RailButton { text: "Titles"; icon: "text"; }
                    RailButton { text: "Audio"; icon: "audio"; }
                    RailButton { text: "Effects"; icon: "wand"; }
                    RailButton { text: "Color"; icon: "image"; }
                }
            }
            Rectangle { width: 1px; background: Theme.palette().border; }
''',
    "",
    "Video inactive workspace rail",
)
video_path.write_text(video.rstrip() + "\n")

# Studio: the inspector does not yet switch independent tab content. Remove the
# cosmetic tab control and label the real channel strip directly.
studio_path = ROOT / "loom-studio/crates/loom-studio-app/ui/app.slint"
studio = studio_path.read_text()
studio = studio.replace(", MetricChip, PaneTabs }", ", MetricChip }")
studio = replace_once(
    studio,
    '''                    PaneTabs { items: ["Channel", "I/O", "Bounce"]; current: 0; }
''',
    '''                    Text {
                        text: "CHANNEL STRIP";
                        font-size: Theme.tokens.typography.caption;
                        font-weight: 700;
                        color: Theme.palette().ink-secondary;
                    }
''',
    "Studio cosmetic inspector tabs",
)
studio_path.write_text(studio.rstrip() + "\n")
