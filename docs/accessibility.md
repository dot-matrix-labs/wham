# Accessibility

This library renders all UI on a GPU canvas and does not use DOM widgets. Accessibility is provided via a parallel tree (`A11yTree`) built from the same widget data.

## Screen Readers
- Use `A11yTree` to build an offscreen accessibility layer in the host environment.
- Each node includes role, name, value, bounds, and states.

## Keyboard Navigation
- Widgets are ordered as they appear in the immediate-mode call order.
- `Tab` navigation is handled in the core; focus state is exposed in the tree.

## High Contrast and Scaling
- Increase `Theme.font_scale` for larger text.
- Use `Theme.high_contrast` to apply a high-contrast palette.

## IME
- Composition events are supported with `CompositionStart/Update/End`.

