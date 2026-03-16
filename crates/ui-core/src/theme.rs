use crate::types::Color;

#[derive(Clone, Debug)]
pub struct Theme {
    pub font_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub colors: ThemeColors,
    /// Blend progress between light and dark themes.
    /// 0.0 = fully light, 1.0 = fully dark.
    pub transition_progress: f32,
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
    /// Light theme — the default appearance.
    pub fn light() -> Self {
        Self {
            font_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            transition_progress: 0.0,
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

    /// Dark theme — suitable for `prefers-color-scheme: dark`.
    pub fn dark() -> Self {
        Self {
            font_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            transition_progress: 1.0,
            colors: ThemeColors {
                // ~#1a1a1a
                background: Color::rgba(0.102, 0.102, 0.102, 1.0),
                // ~#252525
                surface: Color::rgba(0.145, 0.145, 0.145, 1.0),
                // ~#e8e8e8
                text: Color::rgba(0.91, 0.91, 0.91, 1.0),
                // ~#909090
                text_muted: Color::rgba(0.565, 0.565, 0.565, 1.0),
                // slightly brighter accent so it pops on dark surfaces
                primary: Color::rgba(0.35, 0.58, 0.98, 1.0),
                error: Color::rgba(0.98, 0.36, 0.36, 1.0),
                success: Color::rgba(0.27, 0.82, 0.38, 1.0),
                focus_ring: Color::rgba(0.35, 0.58, 0.98, 0.85),
            },
        }
    }

    /// Backwards-compatible alias for `Theme::light()`.
    #[inline]
    pub fn default_light() -> Self {
        Self::light()
    }

    /// Linearly interpolate all color channels and scalar fields between two
    /// themes.  `t = 0.0` returns a copy of `a`; `t = 1.0` returns a copy of
    /// `b`.  `t` is clamped to `[0.0, 1.0]`.
    pub fn interpolate(a: &Theme, b: &Theme, t: f32) -> Theme {
        let t = t.clamp(0.0, 1.0);
        Theme {
            font_scale: lerp(a.font_scale, b.font_scale, t),
            high_contrast: if t < 0.5 { a.high_contrast } else { b.high_contrast },
            reduced_motion: if t < 0.5 { a.reduced_motion } else { b.reduced_motion },
            transition_progress: t,
            colors: ThemeColors {
                background: lerp_color(a.colors.background, b.colors.background, t),
                surface: lerp_color(a.colors.surface, b.colors.surface, t),
                text: lerp_color(a.colors.text, b.colors.text, t),
                text_muted: lerp_color(a.colors.text_muted, b.colors.text_muted, t),
                primary: lerp_color(a.colors.primary, b.colors.primary, t),
                error: lerp_color(a.colors.error, b.colors.error, t),
                success: lerp_color(a.colors.success, b.colors.success, t),
                focus_ring: lerp_color(a.colors.focus_ring, b.colors.focus_ring, t),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::rgba(
        lerp(a.r, b.r, t),
        lerp(a.g, b.g, t),
        lerp(a.b, b.b, t),
        lerp(a.a, b.a, t),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn light_defaults() {
        let t = Theme::light();
        assert!(approx_eq(t.transition_progress, 0.0));
        assert!(!t.reduced_motion);
        assert!(!t.high_contrast);
        // Background should be close to white.
        assert!(t.colors.background.r > 0.9);
        assert!(t.colors.background.b > 0.9);
        // Text should be dark.
        assert!(t.colors.text.r < 0.2);
    }

    #[test]
    fn dark_defaults() {
        let t = Theme::dark();
        assert!(approx_eq(t.transition_progress, 1.0));
        assert!(!t.reduced_motion);
        // Background should be dark.
        assert!(t.colors.background.r < 0.2);
        // Text should be light.
        assert!(t.colors.text.r > 0.8);
    }

    #[test]
    fn default_light_alias() {
        let a = Theme::default_light();
        let b = Theme::light();
        assert!(approx_eq(a.transition_progress, b.transition_progress));
        assert!(approx_eq(a.colors.background.r, b.colors.background.r));
    }

    #[test]
    fn interpolate_at_zero_matches_a() {
        let a = Theme::light();
        let b = Theme::dark();
        let mid = Theme::interpolate(&a, &b, 0.0);
        assert!(approx_eq(mid.colors.background.r, a.colors.background.r));
        assert!(approx_eq(mid.colors.text.r, a.colors.text.r));
        assert!(approx_eq(mid.transition_progress, 0.0));
    }

    #[test]
    fn interpolate_at_one_matches_b() {
        let a = Theme::light();
        let b = Theme::dark();
        let mid = Theme::interpolate(&a, &b, 1.0);
        assert!(approx_eq(mid.colors.background.r, b.colors.background.r));
        assert!(approx_eq(mid.colors.text.r, b.colors.text.r));
        assert!(approx_eq(mid.transition_progress, 1.0));
    }

    #[test]
    fn interpolate_midpoint() {
        let a = Theme::light();
        let b = Theme::dark();
        let mid = Theme::interpolate(&a, &b, 0.5);
        let expected_bg_r = (a.colors.background.r + b.colors.background.r) * 0.5;
        assert!(approx_eq(mid.colors.background.r, expected_bg_r));
        assert!(approx_eq(mid.transition_progress, 0.5));
    }

    #[test]
    fn interpolate_clamps_t() {
        let a = Theme::light();
        let b = Theme::dark();
        let over = Theme::interpolate(&a, &b, 2.0);
        assert!(approx_eq(over.transition_progress, 1.0));
        let under = Theme::interpolate(&a, &b, -1.0);
        assert!(approx_eq(under.transition_progress, 0.0));
    }

    #[test]
    fn reduced_motion_flag_propagates() {
        let mut a = Theme::light();
        a.reduced_motion = true;
        let b = Theme::dark();
        // At t < 0.5 reduced_motion comes from a.
        let mid = Theme::interpolate(&a, &b, 0.3);
        assert!(mid.reduced_motion);
        // At t >= 0.5 reduced_motion comes from b.
        let mid2 = Theme::interpolate(&a, &b, 0.7);
        assert!(!mid2.reduced_motion);
    }
}
