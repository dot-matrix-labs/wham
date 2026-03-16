use crate::types::Color;

#[derive(Clone, Debug)]
pub struct Theme {
    pub font_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub colors: ThemeColors,
}

#[derive(Clone, Debug)]
pub struct ThemeColors {
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub text_muted: Color,
    pub primary: Color,
    pub error: Color,
    pub success: Color,
    pub focus_ring: Color,
}

impl Theme {
    pub fn default_light() -> Self {
        Self {
            font_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            colors: ThemeColors {
                background: Color::rgba(0.97, 0.97, 0.96, 1.0),
                surface: Color::rgba(1.0, 1.0, 1.0, 1.0),
                text: Color::rgba(0.1, 0.1, 0.12, 1.0),
                text_muted: Color::rgba(0.4, 0.4, 0.45, 1.0),
                primary: Color::rgba(0.2, 0.45, 0.9, 1.0),
                error: Color::rgba(0.88, 0.2, 0.2, 1.0),
                success: Color::rgba(0.2, 0.7, 0.3, 1.0),
                focus_ring: Color::rgba(0.2, 0.45, 0.9, 0.8),
            },
        }
    }
}

