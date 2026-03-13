# Theming

The renderer is GPU-only; theming is applied by the Rust core and emitted as batched quads and text runs.

## Theme Tokens
`Theme` defines:
- `font_scale` for text resizing.
- `high_contrast` for accessibility.
- Color tokens: background, surface, text, muted text, primary, error, success.

## High-Contrast
Set `Theme.high_contrast = true` and increase contrast ratios in `ThemeColors`. Use larger `font_scale` for readability.

## Mobile
Increase `font_scale` to 1.2–1.4 and reduce line spacing where needed.

