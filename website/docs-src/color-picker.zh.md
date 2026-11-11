`color_picker(c, "id", &mut color, &mut state)` 绘制 28px 色块，打开预设调色板与 hex 输入。预设点击或合法 hex 设置 `Color`；非法 hex 回退当前颜色。

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

色块发出 `Role::Button`，label 为 `"color"`，value 为 hex 字符串，带 focused 布尔。打开的调色板是 `Role::Popup`；预设格发出 `Role::Button`，label 为 `"preset 0"`…，无 value。hex input 发出 `Role::TextField`，label 为 `"text field"`。

没有 ColorPicker role、rgba/hsv value、alpha、eyedropper、invalid/error 元数据或 AT 桥。
