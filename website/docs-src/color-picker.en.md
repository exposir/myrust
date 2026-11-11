`color_picker(c, "id", &mut color, &mut state)` draws a 28px swatch that opens a preset palette and hex field. Preset click or valid hex sets `Color`; invalid hex falls back to the current color.

```rust
use myrust_core::ui::{color_picker, Color, ColorPickerState};

let mut color = Color::rgb(34, 211, 238);
let mut state = ColorPickerState::default();
let response = color_picker(c, "accent", &mut color, &mut state);
if let Some(index) = response.preset {
    app.use_preset(index);
}
```

## Accessibility

The swatch emits `Role::Button`, label `"color"`, value = hex string, focused boolean. The open palette is `Role::Popup`; preset cells emit `Role::Button`, labels `"preset 0"`…, no value. The hex input emits `Role::TextField`, label `"text field"`.

There is no ColorPicker role, rgba/hsv value, alpha, eyedropper, invalid/error metadata or AT bridge.
