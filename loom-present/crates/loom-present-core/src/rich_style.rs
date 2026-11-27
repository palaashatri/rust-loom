use serde::{Deserialize, Serialize};

/// Drop shadow effect configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropShadowConfig {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: String,
}

impl Default for DropShadowConfig {
    fn default() -> Self {
        Self {
            offset_x: 2.0,
            offset_y: 2.0,
            blur_radius: 4.0,
            color: "rgba(0, 0, 0, 0.3)".into(),
        }
    }
}

/// Stroke dash pattern for element borders and shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StrokeDash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// A single color stop in a linear gradient fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Normalized position in `0.0..=1.0`.
    pub position: f32,
    pub color: String,
}

/// Linear gradient fill configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientFill {
    pub angle_deg: f32,
    pub stops: Vec<GradientStop>,
}

impl Default for GradientFill {
    fn default() -> Self {
        Self {
            angle_deg: 180.0,
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: "#3b82f6".to_string(),
                },
                GradientStop {
                    position: 1.0,
                    color: "#1d4ed8".to_string(),
                },
            ],
        }
    }
}

fn default_opacity() -> f32 {
    1.0
}

/// Rich visual effects attached to a `SlideElement`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementEffects {
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub corner_radius: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    #[serde(default)]
    pub stroke_width: f32,
    #[serde(default)]
    pub stroke_dash: StrokeDash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<DropShadowConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<GradientFill>,
}

impl Default for ElementEffects {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            corner_radius: 0.0,
            stroke_color: None,
            stroke_width: 0.0,
            stroke_dash: StrokeDash::Solid,
            shadow: None,
            gradient: None,
        }
    }
}

impl ElementEffects {
    pub fn is_default(&self) -> bool {
        (self.opacity - 1.0).abs() < f32::EPSILON
            && self.corner_radius.abs() < f32::EPSILON
            && self.stroke_color.is_none()
            && self.stroke_width.abs() < f32::EPSILON
            && self.stroke_dash == StrokeDash::Solid
            && self.shadow.is_none()
            && self.gradient.is_none()
    }
}

impl Default for crate::SlideElement {
    fn default() -> Self {
        Self {
            id: String::new(),
            element_type: crate::ElementType::BodyText,
            content: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            rotation_deg: 0.0,
            action: None,
            style: Default::default(),
            runs: Vec::new(),
            effects: None,
            group_id: None,
            locked: false,
            flip_h: false,
            flip_v: false,
            paragraphs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_effects_is_default() {
        let e = ElementEffects::default();
        assert!(e.is_default());
        assert_eq!(e.opacity, 1.0);
        assert_eq!(e.stroke_dash, StrokeDash::Solid);
    }

    #[test]
    fn effects_roundtrip_serde() {
        let e = ElementEffects {
            opacity: 0.75,
            corner_radius: 12.0,
            stroke_color: Some("#112233".into()),
            stroke_width: 2.5,
            stroke_dash: StrokeDash::Dashed,
            shadow: Some(DropShadowConfig::default()),
            gradient: Some(GradientFill::default()),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: ElementEffects = serde_json::from_str(&json).unwrap();
        assert_eq!(e, parsed);
    }
}
