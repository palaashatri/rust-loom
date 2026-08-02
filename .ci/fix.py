from __future__ import annotations

from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
UI = ROOT / "loom-core/crates/loom-ui/ui"

THEME = r'''// Loom creator-grade design tokens.
// Original visual language: graphite materials, warm copper focus, restrained
// depth, dense professional controls, and deterministic accessible themes.

struct Palette {
    canvas: color,
    canvas-alt: color,
    surface: color,
    surface-raised: color,
    surface-sunken: color,
    chrome: color,
    panel: color,
    overlay: color,
    ink: color,
    ink-secondary: color,
    ink-disabled: color,
    accent: color,
    accent-hover: color,
    accent-pressed: color,
    accent-ink: color,
    success: color,
    warning: color,
    danger: color,
    info: color,
    border: color,
    border-strong: color,
    focus: color,
    selection: color,
    selection-soft: color,
    danger-surface: color,
    shadow: color,
    scrim: color,
    grid-major: color,
    grid-minor: color,
}

struct TypeScale {
    micro: length,
    caption: length,
    body: length,
    body-large: length,
    title: length,
    heading: length,
    display: length,
    display-large: length,
}

struct Spacing {
    xs: length,
    sm: length,
    md: length,
    lg: length,
    xl: length,
    xxl: length,
    xxxl: length,
    xxxxl: length,
}

struct Motion {
    instant-ms: int,
    fast-ms: int,
    standard-ms: int,
    deliberate-ms: int,
    ease-out-x1: float,
    ease-out-y1: float,
    ease-out-x2: float,
    ease-out-y2: float,
}

struct Metrics {
    control-height: length,
    compact-control-height: length,
    toolbar-height: length,
    header-height: length,
    panel-header-height: length,
    radius-small: length,
    radius-medium: length,
    radius-large: length,
}

struct ThemeTokens {
    palette: Palette,
    typography: TypeScale,
    space: Spacing,
    motion: Motion,
    metrics: Metrics,
    reduced-motion: bool,
}

export global Theme {
    in-out property <ThemeTokens> tokens: {
        palette: {
            canvas: #ececea,
            canvas-alt: #e4e4e1,
            surface: #f8f8f6,
            surface-raised: #ffffff,
            surface-sunken: #e1e1de,
            chrome: #f1f1ef,
            panel: #f5f5f3,
            overlay: #fffffff2,
            ink: #18191c,
            ink-secondary: #5c5e64,
            ink-disabled: #92949a,
            accent: #b85c34,
            accent-hover: #ca6a40,
            accent-pressed: #9f4d2b,
            accent-ink: #ffffff,
            success: #347a50,
            warning: #9d651e,
            danger: #b33d35,
            info: #3e6f9c,
            border: #d0d0cc,
            border-strong: #adada8,
            focus: #d7794e,
            selection: #b85c34,
            selection-soft: #eed8cc,
            danger-surface: #f5ddda,
            shadow: #00000024,
            scrim: #00000066,
            grid-major: #bdbdb8,
            grid-minor: #d9d9d5,
        },
        typography: { micro: 10px, caption: 11px, body: 13px, body-large: 14px, title: 16px, heading: 22px, display: 28px, display-large: 36px },
        space: { xs: 2px, sm: 4px, md: 8px, lg: 12px, xl: 16px, xxl: 24px, xxxl: 32px, xxxxl: 48px },
        motion: { instant-ms: 70, fast-ms: 120, standard-ms: 190, deliberate-ms: 300, ease-out-x1: 0.2, ease-out-y1: 0.9, ease-out-x2: 0.2, ease-out-y2: 1.0 },
        metrics: { control-height: 32px, compact-control-height: 28px, toolbar-height: 44px, header-height: 58px, panel-header-height: 38px, radius-small: 6px, radius-medium: 9px, radius-large: 13px },
        reduced-motion: false,
    };

    in-out property <string> active-theme: "light";

    public pure function palette() -> Palette {
        return active-theme == "dark" ? ThemeDark.tokens.palette
            : active-theme == "high-contrast" ? ThemeHighContrast.tokens.palette
            : tokens.palette;
    }
}

export global ThemeDark {
    in-out property <ThemeTokens> tokens: {
        palette: {
            canvas: #0b0c0f,
            canvas-alt: #0f1014,
            surface: #17181c,
            surface-raised: #202126,
            surface-sunken: #111216,
            chrome: #131418,
            panel: #18191d,
            overlay: #202126f2,
            ink: #f4f3ef,
            ink-secondary: #a8a8ad,
            ink-disabled: #6d6f76,
            accent: #d97845,
            accent-hover: #eb8a57,
            accent-pressed: #bd6236,
            accent-ink: #160d08,
            success: #66bb84,
            warning: #dfa553,
            danger: #e27065,
            info: #7aabd6,
            border: #292b31,
            border-strong: #44474f,
            focus: #f29a6a,
            selection: #d97845,
            selection-soft: #3a271f,
            danger-surface: #3b2222,
            shadow: #00000080,
            scrim: #00000099,
            grid-major: #35373d,
            grid-minor: #24262b,
        },
        typography: { micro: 10px, caption: 11px, body: 13px, body-large: 14px, title: 16px, heading: 22px, display: 28px, display-large: 36px },
        space: { xs: 2px, sm: 4px, md: 8px, lg: 12px, xl: 16px, xxl: 24px, xxxl: 32px, xxxxl: 48px },
        motion: { instant-ms: 70, fast-ms: 120, standard-ms: 190, deliberate-ms: 300, ease-out-x1: 0.2, ease-out-y1: 0.9, ease-out-x2: 0.2, ease-out-y2: 1.0 },
        metrics: { control-height: 32px, compact-control-height: 28px, toolbar-height: 44px, header-height: 58px, panel-header-height: 38px, radius-small: 6px, radius-medium: 9px, radius-large: 13px },
        reduced-motion: false,
    };
}

export global ThemeHighContrast {
    in-out property <ThemeTokens> tokens: {
        palette: {
            canvas: #000000,
            canvas-alt: #000000,
            surface: #000000,
            surface-raised: #161616,
            surface-sunken: #0b0b0b,
            chrome: #000000,
            panel: #000000,
            overlay: #000000f2,
            ink: #ffffff,
            ink-secondary: #ffffff,
            ink-disabled: #c8c8c8,
            accent: #ffd84d,
            accent-hover: #ffe681,
            accent-pressed: #e7bd25,
            accent-ink: #000000,
            success: #80ff80,
            warning: #ffbd52,
            danger: #ff7164,
            info: #80ccff,
            border: #ffffff,
            border-strong: #ffffff,
            focus: #ffd84d,
            selection: #ffd84d,
            selection-soft: #3a3200,
            danger-surface: #3d0d09,
            shadow: #000000,
            scrim: #000000cc,
            grid-major: #ffffff,
            grid-minor: #777777,
        },
        typography: { micro: 10px, caption: 11px, body: 13px, body-large: 14px, title: 16px, heading: 22px, display: 28px, display-large: 36px },
        space: { xs: 2px, sm: 4px, md: 8px, lg: 12px, xl: 16px, xxl: 24px, xxxl: 32px, xxxxl: 48px },
        motion: { instant-ms: 0, fast-ms: 0, standard-ms: 0, deliberate-ms: 0, ease-out-x1: 0.0, ease-out-y1: 1.0, ease-out-x2: 1.0, ease-out-y2: 1.0 },
        metrics: { control-height: 34px, compact-control-height: 30px, toolbar-height: 46px, header-height: 60px, panel-header-height: 40px, radius-small: 4px, radius-medium: 6px, radius-large: 8px },
        reduced-motion: true,
    };
}
'''


def replace_component(text: str, name: str, replacement: str) -> str:
    marker = f"export component {name}"
    start = text.find(marker)
    if start < 0:
        raise RuntimeError(f"missing component {name}")
    brace = text.find("{", start)
    if brace < 0:
        raise RuntimeError(f"missing opening brace for {name}")
    depth = 0
    i = brace
    string = False
    escape = False
    line_comment = False
    block_comment = False
    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""
        if line_comment:
            if ch == "\n":
                line_comment = False
        elif block_comment:
            if ch == "*" and nxt == "/":
                block_comment = False
                i += 1
        elif string:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == '"':
                string = False
        else:
            if ch == "/" and nxt == "/":
                line_comment = True
                i += 1
            elif ch == "/" and nxt == "*":
                block_comment = True
                i += 1
            elif ch == '"':
                string = True
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    return text[:start] + replacement.strip() + "\n" + text[i + 1 :]
        i += 1
    raise RuntimeError(f"unterminated component {name}")


COMPONENTS = {
"StatusBar": r'''export component StatusBar inherits Rectangle {
    in property <string> status-left: "";
    in property <string> status-right: "";
    min-height: 30px;
    max-height: 30px;
    background: Theme.palette().chrome;
    Rectangle { y: 0px; height: 1px; width: parent.width; background: Theme.palette().border; }
    HorizontalLayout {
        spacing: 12px;
        padding-left: 14px;
        padding-right: 14px;
        cross-axis-alignment: center;
        Rectangle { width: 5px; height: 5px; border-radius: 3px; background: Theme.palette().accent; }
        Text { text: root.status-left; font-size: Theme.tokens.typography.caption; color: Theme.palette().ink-secondary; horizontal-stretch: 1; vertical-alignment: center; }
        Text { text: root.status-right; font-size: Theme.tokens.typography.caption; font-weight: 600; color: Theme.palette().ink-disabled; vertical-alignment: center; }
    }
}''',
"ToolButton": r'''export component ToolButton inherits Rectangle {
    in property <string> text: "";
    in property <string> icon: "";
    in property <bool> checked: false;
    in property <bool> enabled: true;
    callback clicked;
    height: Theme.tokens.metrics.control-height;
    min-width: Theme.tokens.metrics.control-height;
    border-radius: Theme.tokens.metrics.radius-medium;
    border-width: 1px;
    border-color: !root.enabled ? transparent : focus-scope.has-focus ? Theme.palette().focus : root.checked ? Theme.palette().accent : touch.has-hover ? Theme.palette().border : transparent;
    background: !root.enabled ? transparent : touch.pressed ? Theme.palette().surface-sunken : root.checked ? Theme.palette().selection-soft : touch.has-hover ? Theme.palette().surface-raised : transparent;
    animate background, border-color { duration: Theme.tokens.reduced-motion ? 0ms : 120ms; easing: cubic-bezier(0.2, 0.9, 0.2, 1.0); }
    accessible-role: button;
    accessible-label: root.text != "" ? root.text : root.icon;
    accessible-action-default => { if (root.enabled) { root.clicked(); } }
    focus-scope := FocusScope { enabled: root.enabled; key-pressed(event) => { if (root.enabled && (event.text == Key.Space || event.text == Key.Return)) { root.clicked(); return accept; } return reject; } }
    HorizontalLayout {
        spacing: 6px;
        alignment: center;
        padding-left: root.icon != "" ? 8px : 11px;
        padding-right: root.text != "" ? 11px : 8px;
        if root.icon != "" : Icon { icon: root.icon; size: 15px; tint: !root.enabled ? Theme.palette().ink-disabled : root.checked ? Theme.palette().accent : Theme.palette().ink-secondary; }
        if root.text != "" : Text { text: root.text; font-size: Theme.tokens.typography.body; font-weight: root.checked ? 650 : 500; color: root.enabled ? Theme.palette().ink : Theme.palette().ink-disabled; }
    }
    if root.checked : Rectangle { x: 8px; y: parent.height - 3px; width: parent.width - 16px; height: 2px; border-radius: 1px; background: Theme.palette().accent; }
    touch := TouchArea { enabled: root.enabled; clicked => { focus-scope.focus(); root.clicked(); } }
}''',
"IconButton": r'''export component IconButton inherits Rectangle {
    in property <string> icon: "generic";
    in property <string> label: "";
    in property <bool> enabled: true;
    in property <bool> checked: false;
    callback clicked;
    width: Theme.tokens.metrics.control-height;
    height: Theme.tokens.metrics.control-height;
    border-radius: Theme.tokens.metrics.radius-medium;
    border-width: 1px;
    border-color: !root.enabled ? transparent : focus-scope.has-focus ? Theme.palette().focus : root.checked ? Theme.palette().accent : touch.has-hover ? Theme.palette().border : transparent;
    background: !root.enabled ? transparent : touch.pressed ? Theme.palette().surface-sunken : root.checked ? Theme.palette().selection-soft : touch.has-hover ? Theme.palette().surface-raised : transparent;
    animate background, border-color { duration: Theme.tokens.reduced-motion ? 0ms : 120ms; easing: cubic-bezier(0.2, 0.9, 0.2, 1.0); }
    accessible-role: button;
    accessible-label: root.label != "" ? root.label : root.icon;
    accessible-action-default => { if (root.enabled) { root.clicked(); } }
    focus-scope := FocusScope { enabled: root.enabled; key-pressed(event) => { if (root.enabled && (event.text == Key.Space || event.text == Key.Return)) { root.clicked(); return accept; } return reject; } }
    Icon { icon: root.icon; size: 16px; tint: !root.enabled ? Theme.palette().ink-disabled : root.checked ? Theme.palette().accent : Theme.palette().ink-secondary; }
    touch := TouchArea { enabled: root.enabled; clicked => { focus-scope.focus(); root.clicked(); } }
}''',
"PrimaryButton": r'''export component PrimaryButton inherits Rectangle {
    in property <string> text: "";
    in property <bool> enabled: true;
    callback clicked;
    height: Theme.tokens.metrics.control-height;
    min-width: 78px;
    border-radius: Theme.tokens.metrics.radius-medium;
    background: !root.enabled ? Theme.palette().surface-sunken : touch.pressed ? Theme.palette().accent-pressed : touch.has-hover ? Theme.palette().accent-hover : Theme.palette().accent;
    border-width: 1px;
    border-color: focus-scope.has-focus ? Theme.palette().focus : root.enabled ? Theme.palette().accent-hover : Theme.palette().border;
    animate background, border-color { duration: Theme.tokens.reduced-motion ? 0ms : 120ms; easing: cubic-bezier(0.2, 0.9, 0.2, 1.0); }
    accessible-role: button;
    accessible-label: root.text;
    accessible-action-default => { if (root.enabled) { root.clicked(); } }
    focus-scope := FocusScope { enabled: root.enabled; key-pressed(event) => { if (root.enabled && (event.text == Key.Space || event.text == Key.Return)) { root.clicked(); return accept; } return reject; } }
    Text { text: root.text; font-size: Theme.tokens.typography.body; font-weight: 650; color: root.enabled ? Theme.palette().accent-ink : Theme.palette().ink-disabled; }
    touch := TouchArea { enabled: root.enabled; clicked => { focus-scope.focus(); root.clicked(); } }
}''',
"AppHeader": r'''export component AppHeader inherits Rectangle {
    in property <string> app-name: "Loom";
    in property <string> document-title: "Untitled";
    in property <string> icon: "layers";
    in property <string> context: "";
    in property <string> state-text: "Local";
    in property <int> state-tone: 0;
    min-height: Theme.tokens.metrics.header-height;
    max-height: Theme.tokens.metrics.header-height;
    background: Theme.palette().chrome;
    VerticalLayout {
        spacing: 0px;
        HorizontalLayout {
            spacing: 11px;
            padding-left: 14px;
            padding-right: 14px;
            cross-axis-alignment: center;
            Rectangle {
                width: 34px; height: 34px; border-radius: 11px;
                background: Theme.palette().selection-soft;
                border-width: 1px; border-color: Theme.palette().accent;
                Icon { icon: root.icon; size: 18px; tint: Theme.palette().accent; }
            }
            VerticalLayout {
                spacing: 1px;
                Text { text: root.document-title; font-size: Theme.tokens.typography.title; font-weight: 650; color: Theme.palette().ink; }
                Text { text: root.app-name; font-size: Theme.tokens.typography.caption; font-weight: 600; color: Theme.palette().ink-secondary; }
            }
            Rectangle { horizontal-stretch: 1; background: transparent; }
            if root.context != "" : Rectangle {
                height: 28px; min-width: 96px; border-radius: 8px; background: Theme.palette().surface-sunken; border-width: 1px; border-color: Theme.palette().border;
                Text { text: root.context; font-size: Theme.tokens.typography.caption; color: Theme.palette().ink-secondary; horizontal-alignment: center; vertical-alignment: center; }
            }
            StatusPill { text: root.state-text; tone: root.state-tone; }
        }
        Rectangle { height: 1px; background: Theme.palette().border; }
    }
}''',
"PanelHeader": r'''export component PanelHeader inherits Rectangle {
    in property <string> title: "Panel";
    in property <string> detail: "";
    min-height: Theme.tokens.metrics.panel-header-height;
    max-height: Theme.tokens.metrics.panel-header-height;
    background: Theme.palette().panel;
    HorizontalLayout {
        spacing: 8px; padding-left: 12px; padding-right: 10px; cross-axis-alignment: center;
        Rectangle { width: 3px; height: 14px; border-radius: 2px; background: Theme.palette().accent; }
        Text { text: root.title; font-size: Theme.tokens.typography.caption; font-weight: 700; color: Theme.palette().ink; }
        Rectangle { horizontal-stretch: 1; background: transparent; }
        if root.detail != "" : Text { text: root.detail; font-size: Theme.tokens.typography.micro; font-weight: 600; color: Theme.palette().ink-disabled; }
    }
    Rectangle { y: parent.height - 1px; height: 1px; width: parent.width; background: Theme.palette().border; }
}''',
"RailButton": r'''export component RailButton inherits Rectangle {
    in property <string> text: "";
    in property <string> icon: "generic";
    in property <bool> checked: false;
    in property <bool> enabled: true;
    callback clicked;
    width: 60px; height: 54px; border-radius: 10px; border-width: 1px;
    border-color: focus-scope.has-focus ? Theme.palette().focus : root.checked ? Theme.palette().border-strong : transparent;
    background: root.checked ? Theme.palette().surface-raised : touch.has-hover ? Theme.palette().surface-sunken : transparent;
    animate background, border-color { duration: Theme.tokens.reduced-motion ? 0ms : 120ms; easing: cubic-bezier(0.2, 0.9, 0.2, 1.0); }
    accessible-role: button; accessible-label: root.text; accessible-action-default => { if (root.enabled) { root.clicked(); } }
    focus-scope := FocusScope { enabled: root.enabled; key-pressed(event) => { if (root.enabled && (event.text == Key.Space || event.text == Key.Return)) { root.clicked(); return accept; } return reject; } }
    if root.checked : Rectangle { x: 0px; y: 12px; width: 3px; height: parent.height - 24px; border-radius: 2px; background: Theme.palette().accent; }
    VerticalLayout {
        spacing: 4px; alignment: center;
        Icon { icon: root.icon; size: 18px; tint: !root.enabled ? Theme.palette().ink-disabled : root.checked ? Theme.palette().accent : Theme.palette().ink-secondary; }
        Text { text: root.text; font-size: Theme.tokens.typography.micro; font-weight: root.checked ? 700 : 550; color: !root.enabled ? Theme.palette().ink-disabled : root.checked ? Theme.palette().ink : Theme.palette().ink-secondary; horizontal-alignment: center; }
    }
    touch := TouchArea { enabled: root.enabled; clicked => { focus-scope.focus(); root.clicked(); } }
}''',
"WorkspaceToolbar": r'''export component WorkspaceToolbar inherits Rectangle {
    min-height: Theme.tokens.metrics.toolbar-height;
    max-height: Theme.tokens.metrics.toolbar-height;
    background: Theme.palette().chrome;
    VerticalLayout {
        spacing: 0px;
        HorizontalLayout { spacing: 5px; padding-left: 10px; padding-right: 10px; cross-axis-alignment: center; @children }
        Rectangle { height: 1px; background: Theme.palette().border; }
    }
}''',
"SidebarSurface": r'''export component SidebarSurface inherits Rectangle {
    in property <string> title: "";
    in property <string> detail: "";
    background: Theme.palette().panel;
    border-width: 0px;
    VerticalLayout { spacing: 0px; PanelHeader { title: root.title; detail: root.detail; } @children }
}''',
"InspectorSurface": r'''export component InspectorSurface inherits Rectangle {
    in property <string> title: "Inspector";
    in property <string> detail: "";
    background: Theme.palette().panel;
    border-width: 0px;
    VerticalLayout { spacing: 0px; PanelHeader { title: root.title; detail: root.detail; } @children }
}''',
"WorkspaceRow": r'''export component WorkspaceRow inherits Rectangle {
    in property <bool> selected: false;
    in property <bool> emphasized: false;
    in property <bool> enabled: true;
    callback clicked;
    min-height: 42px; border-radius: 9px; border-width: 1px;
    border-color: root.selected ? Theme.palette().accent : root.emphasized ? Theme.palette().border-strong : transparent;
    background: root.selected ? Theme.palette().selection-soft : touch.has-hover && root.enabled ? Theme.palette().surface-raised : transparent;
    animate background, border-color { duration: Theme.tokens.reduced-motion ? 0ms : 120ms; easing: cubic-bezier(0.2, 0.9, 0.2, 1.0); }
    if root.selected : Rectangle { x: 0px; y: 8px; width: 3px; height: parent.height - 16px; border-radius: 2px; background: Theme.palette().accent; }
    @children
    touch := TouchArea { enabled: root.enabled; clicked => { root.clicked(); } }
}''',
"CanvasBackdrop": r'''export component CanvasBackdrop inherits Rectangle {
    in property <color> stage-color: Theme.palette().surface-sunken;
    background: Theme.palette().canvas-alt;
    Rectangle { x: 17px; y: 18px; width: parent.width - 34px; height: parent.height - 34px; border-radius: 14px; background: Theme.palette().shadow; }
    Rectangle {
        x: 12px; y: 12px; width: parent.width - 24px; height: parent.height - 24px;
        border-radius: 13px; border-width: 1px; border-color: Theme.palette().border-strong; background: root.stage-color; clip: true; @children
    }
}''',
"PaneTabs": r'''export component PaneTabs inherits Rectangle {
    in property <[string]> items: [];
    in-out property <int> current: 0;
    callback selected(int);
    height: 36px; background: Theme.palette().panel;
    HorizontalLayout {
        spacing: 3px; padding-left: 8px; padding-right: 8px; padding-top: 4px; padding-bottom: 4px;
        for item[idx] in root.items : Rectangle {
            min-width: 56px; border-radius: 8px;
            background: idx == root.current ? Theme.palette().surface-raised : tab-touch.has-hover ? Theme.palette().surface-sunken : transparent;
            border-width: 1px; border-color: idx == root.current ? Theme.palette().border-strong : transparent;
            Text { text: item; font-size: Theme.tokens.typography.caption; font-weight: idx == root.current ? 700 : 500; color: idx == root.current ? Theme.palette().ink : Theme.palette().ink-secondary; horizontal-alignment: center; vertical-alignment: center; }
            if idx == root.current : Rectangle { y: parent.height - 3px; x: 10px; width: parent.width - 20px; height: 2px; border-radius: 1px; background: Theme.palette().accent; }
            tab-touch := TouchArea { clicked => { root.current = idx; root.selected(idx); } }
        }
    }
}''',
"TransportButton": r'''export component TransportButton inherits Rectangle {
    in property <string> icon: "play";
    in property <string> label: "";
    in property <bool> active: false;
    in property <bool> destructive: false;
    in property <bool> enabled: true;
    callback clicked;
    width: 36px; height: 36px; border-radius: 18px; border-width: 1px;
    border-color: root.active ? (root.destructive ? Theme.palette().danger : Theme.palette().accent) : touch.has-hover ? Theme.palette().border-strong : Theme.palette().border;
    background: root.active ? (root.destructive ? Theme.palette().danger-surface : Theme.palette().selection-soft) : touch.pressed ? Theme.palette().surface-sunken : touch.has-hover ? Theme.palette().surface-raised : transparent;
    animate background, border-color { duration: Theme.tokens.reduced-motion ? 0ms : 120ms; easing: cubic-bezier(0.2, 0.9, 0.2, 1.0); }
    accessible-role: button; accessible-label: root.label;
    Icon { icon: root.icon; size: 16px; tint: !root.enabled ? Theme.palette().ink-disabled : root.destructive ? Theme.palette().danger : root.active ? Theme.palette().accent : Theme.palette().ink; }
    touch := TouchArea { enabled: root.enabled; clicked => { root.clicked(); } }
}''',
"EmptyState": r'''export component EmptyState inherits Rectangle {
    in property <string> icon: "layers";
    in property <string> title: "";
    in property <string> description: "";
    in property <string> action-text: "";
    callback action;
    background: transparent; max-width: 10000px; max-height: 10000px;
    Rectangle {
        width: min(480px, parent.width - 32px); height: min(280px, parent.height - 32px); border-radius: 16px;
        background: Theme.palette().surface; border-width: 1px; border-color: Theme.palette().border;
        VerticalLayout {
            spacing: 10px; padding: 28px; alignment: center; cross-axis-alignment: center;
            Rectangle { width: 54px; height: 54px; border-radius: 18px; background: Theme.palette().selection-soft; Icon { icon: root.icon; size: 25px; tint: Theme.palette().accent; } }
            Text { text: root.title; font-size: Theme.tokens.typography.heading; font-weight: 650; color: Theme.palette().ink; horizontal-alignment: center; }
            Text { text: root.description; font-size: Theme.tokens.typography.body; color: Theme.palette().ink-secondary; horizontal-alignment: center; wrap: word-wrap; width: 390px; }
            if root.action-text != "" : PrimaryButton { text: root.action-text; clicked => { root.action(); } }
        }
    }
}''',
}


def productize_apps() -> None:
    app_files = [
        ROOT / "loom-writer/crates/loom-writer-app/ui/app.slint",
        ROOT / "loom-sheets/crates/loom-sheets-app/ui/app.slint",
        ROOT / "loom-present/crates/loom-present-app/ui/app.slint",
        ROOT / "loom-photo/crates/loom-photo-app/ui/app.slint",
        ROOT / "loom-motion/crates/loom-motion-app/ui/app.slint",
        ROOT / "loom-video/crates/loom-video-app/ui/app.slint",
        ROOT / "loom-studio/crates/loom-studio-app/ui/app.slint",
        ROOT / "loom-encode/crates/loom-encode-app/ui/app.slint",
    ]
    replacements = {
        "⏮ Step Back": "Step Back",
        "⏭ Step Fwd": "Step Forward",
        "▶ Play": "Play",
        "⏸ Pause": "Pause",
        "🔁 Loop": "Loop",
        "📈 Curve Editor": "Curves",
        "🎬": "",
        "🎵": "",
        "🎚": "",
        "🎛": "",
        "📁": "",
        "📤": "",
        "⚙": "",
        "✨": "",
    }
    emoji = re.compile("[\\U0001F000-\\U0001FAFF\\u2600-\\u27BF]")
    for path in app_files:
        if not path.exists():
            raise RuntimeError(f"missing app UI {path}")
        text = path.read_text()
        for old, new in replacements.items():
            text = text.replace(old, new)
        text = emoji.sub("", text)
        text = text.replace('state-text: "Model preview";', 'state-text: "Local project";')
        text = text.replace('state-text: "CPU preview";', 'state-text: "Local render";')
        text = text.replace('state-text: "Reference engine";', 'state-text: "Local engine";')
        text = re.sub(
            r"min-height: 40px;\s*\n\s*max-height: 40px;\s*\n\s*background: Theme\.palette\(\)\.surface-sunken;",
            "min-height: Theme.tokens.metrics.toolbar-height;\n            max-height: Theme.tokens.metrics.toolbar-height;\n            background: Theme.palette().chrome;",
            text,
        )
        text = text.replace("font-size: 10px;", "font-size: Theme.tokens.typography.micro;")
        text = re.sub(r"[ \t]+\n", "\n", text)
        path.write_text(text)


def write_audit() -> None:
    path = ROOT / "loom-bootstrap/scripts/audit-product-ui.py"
    path.write_text(r'''#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
APPS = ["writer", "sheets", "present", "photo", "motion", "video", "studio", "encode"]
failures = []
emoji = re.compile("[\\U0001F000-\\U0001FAFF\\u2600-\\u27BF]")
for app in APPS:
    file = ROOT / f"loom-{app}/crates/loom-{app}-app/ui/app.slint"
    text = file.read_text()
    if "AppHeader {" not in text:
        failures.append(f"{app}: missing shared AppHeader")
    if "StatusBar {" not in text:
        failures.append(f"{app}: missing shared StatusBar")
    if "Theme.palette()" not in text:
        failures.append(f"{app}: bypasses semantic palette")
    if emoji.search(text):
        failures.append(f"{app}: emoji/icon-font glyphs remain in professional UI")
    if 'state-text: "Model preview"' in text:
        failures.append(f"{app}: legacy prototype status remains")
    if re.search(r"#[0-9a-fA-F]{6,8}", text):
        failures.append(f"{app}: hard-coded color outside the shared theme")

shared = (ROOT / "loom-core/crates/loom-ui/ui/components.slint").read_text()
for component in ["WorkspaceToolbar", "SidebarSurface", "InspectorSurface", "PaneTabs", "CanvasBackdrop", "TransportButton"]:
    if f"export component {component}" not in shared:
        failures.append(f"shared UI: missing {component}")

theme = (ROOT / "loom-core/crates/loom-ui/ui/theme.slint").read_text()
for token in ["surface-raised", "chrome", "panel", "shadow", "grid-major", "control-height", "header-height"]:
    if token not in theme:
        failures.append(f"theme: missing product token {token}")

if failures:
    print("Loom UI productisation audit failed:")
    for failure in failures:
        print(f"- {failure}")
    sys.exit(1)
print("Loom UI productisation audit passed for all eight applications")
''')
    path.chmod(0o755)


def main() -> None:
    (UI / "theme.slint").write_text(THEME)
    components_path = UI / "components.slint"
    text = components_path.read_text()
    for name, definition in COMPONENTS.items():
        text = replace_component(text, name, definition)
    components_path.write_text(text)
    productize_apps()
    write_audit()


if __name__ == "__main__":
    main()
