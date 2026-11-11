transition 把一个 `Rect` 插值到目标。`TransitionState` 由调用方持有；第一次调用直接初始化到目标，目标变化时从当前插值矩形开始新动画。

```rust
use myrust_core::ui::{transition, Rect, TransitionState};

let target = Rect::new(if app.open { 180.0 } else { 0.0 }, 20.0, 120.0, 60.0);
let frame = transition(c, &mut app.panel_transition, target);
container(c).size(frame.bounds.width, frame.bounds.height);
```

返回的 `TransitionFrame` 有 `bounds`、`progress`、`done`、`retargeted`。它不会自动定位子节点；调用方把 bounds 应用到绘制或布局。

## What moves

只有 bounds 动画。颜色、透明度、圆角、边框、阴影、文本内容与 z-order 没有 transition primitive。需要时读取 `frame.progress` 每帧重建 `Color`/命令。

`TransitionState.animation` 设置后续重定向的时长/缓动。`instant_state()` 创建零时长线性状态，用于不要动画的调用方。

## Coming and going

没有 enter/exit 生命周期、mount/unmount 动画或 presence tracking。调用方决定内容何时存在，并必须在退出动画本应结束时仍构建它；否则立即消失。

### Exits decided as they go

UI 不跟踪退出状态。移除依赖布局或滚动时，调用方需在应用代码中决定并让 transition 值继续存活。

### One at a time

没有队列或“同一时间一个 transition”协调器。一个 `TransitionState` 只插值一个矩形；多个元素需要多个状态。

### Timing the exit

没有独立于主动画的退出时长、延迟或取消回调。设置 `TransitionState.animation` 或使用 `instant_state`；时间全部来自该 `Animation`。

## Shared elements

没有 shared-element transition、keyed element 匹配或跨页飞行。调用方可以让一个矩形动画到另一个组件 bounds，但 UI 不会发现或保留 identity。

### Flying

“Flying”需要调用方计算起止 `Rect` 并运行 `transition`；没有框架 matcher 或 overlay clone。

### Everything else

只插值几何。跨容器的调用方必须在插值矩形处自绘内容；source/target 控件不会自动克隆。

### Scaled content

文字与子布局不会整体缩放。transition rect 不应用 `Transform`；需要缩放时调用方用 `Context::transform` 包裹内容。

### Text that morphs

没有文本 morph、交叉淡入或 glyph 插值。文本每帧重建；字符串变化会直接跳变，除非调用方手动淡入或动画颜色。

## When nothing moves

`transition` 第一次直接初始化到目标，新建状态不会动画进入。目标等于 `state.to` 时不重启。Reduce-motion 只有在调用方检查 `Context::reduce_motion` 时才让自定义动画跳过；`transition` 本身仍按其 animation 时长运行。

完全不动画可使用 `instant_state()` 或零时长 `Animation`；没有全局“禁用 transition”开关。
