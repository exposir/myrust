`toggle(c, "id", "Bold", &mut active, group)` 绘制按下态按钮。无 group 时调用方 `bool` 独立切换；有 `ToggleGroup` 时一个 active id 是唯一事实来源。

```rust
use myrust_core::ui::{toggle, ToggleGroup};

let left = toggle(c, "left", "Left", &mut app.left, Some(&mut app.toggle_group));
let right = toggle(c, "right", "Right", &mut app.right, Some(&mut app.toggle_group));
```

## Toggle groups

`ToggleGroup { active: Option<String> }` 最多保留一个 id。激活新 toggle 会清掉前一个；再次点击 active 会清空 group。每个 toggle 当帧把 `bool` 同步到 group。

group 不去重 id、不自动移除缺失成员，也不支持 disabled/三态成员。调用方每帧必须渲染所有成员，bool 才能同步。

## Without a look

没有底层 toggle builder 或自定义 track/thumb。自定义按下控件需 `container`/`draw`、自己的点击处理与 AccessNode 路径；没有公开 widget-base protocol。

## Accessibility

每个 toggle 发出 `Role::Button`，label 为文本，value 为 `"active"`/`"inactive"`，带 focused 布尔。group 本身没有独立 node；没有 pressed/checked role，也没有 AT 桥。
