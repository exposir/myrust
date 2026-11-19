transition 把一个值插值到目标。`TransitionState` 处理 bounds；`ColorTransition`、`OpacityTransition`、`SizeTransition` 与 `PositionTransition` 覆盖其余常见的绘制和布局值。它们都由调用方持有：第一次调用直接初始化到目标，目标变化时从当前插值值开始新动画。

```rust
use myrust_core::ui::{transition, Rect, TransitionState};

let target = Rect::new(if app.open { 180.0 } else { 0.0 }, 20.0, 120.0, 60.0);
let frame = transition(c, &mut app.panel_transition, target);
container(c).size(frame.bounds.width, frame.bounds.height);
```

返回的 `TransitionFrame` 有 `bounds`、`progress`、`done`、`retargeted`。它不会自动定位子节点；调用方把 bounds 应用到绘制或布局。

## What moves

bounds、颜色、透明度、尺寸与位置有 transition primitive。圆角、边框、阴影、文本内容与 z-order 没有；需要时由调用方持有值并每帧重建命令。

```rust
use myrust_core::ui::{color_transition, Animation, Color, ColorTransition};

let mut fade = ColorTransition::new(Animation::default());
let frame = color_transition(c, &mut fade, Color::rgb(34, 211, 238));
let color = frame.value;
```

`OpacityTransition` 是标量，`opacity_transition` 会把目标限制在 `0.0..=1.0`。`SizeTransition` 与 `PositionTransition` 分别插值 `Size` 和 `Point`。它们共用 `Animation` 的时长/缓动，并通过 `TransitionValueFrame` 暴露 `progress`、`done`、`retargeted`。

`TransitionState.animation` 与 `ValueTransition.animation` 设置后续重定向的时长/缓动。`instant_state()` 创建零时长线性 bounds 状态，用于不要动画的调用方。

Transition 目标变化从当前值重定向，但不保留速度。被打断的运动需要惯性时使用 `Spring`。`ValueTransition::step(dt)` 不依赖时钟推进，便于确定性测试。

由 `Context` 管理的标量可用 `animate_spring` / `animate_spring_with`：retarget 保留 velocity，并纳入 `next_animation_frame()`。bounds/color/size/position 记录用 transitions，需要动量时用 springs。

## Coming and going

没有 enter/exit 生命周期、mount/unmount 动画或 presence tracking。调用方决定内容何时存在，并必须在退出动画本应结束时仍构建它；否则立即消失。

### Exits decided as they go

UI 不跟踪退出状态。移除依赖布局或滚动时，调用方需在应用代码中决定并让 transition 值继续存活。

### One at a time

没有队列或“同一时间一个 transition”协调器。一个状态只插值一个值；多个元素需要多个状态。

### Timing the exit

没有独立于主动画的退出时长、延迟或取消回调。设置状态的 `animation` 或使用 `instant_state`；时间全部来自该 `Animation`。

## Shared elements

没有 shared-element transition、keyed element 匹配或跨页飞行。调用方可以让一个矩形动画到另一个组件 bounds，但 UI 不会发现或保留 identity。

### Flying

“Flying”需要调用方计算起止 `Rect` 并运行 `transition`；没有框架 matcher 或 overlay clone。

### Everything else

只插值选定的值。跨容器的调用方必须在插值矩形处自绘内容；source/target 控件不会自动克隆。

### Scaled content

文字与子布局不会整体缩放。transition rect 不应用 `Transform`；需要缩放时调用方用 `Context::transform` 包裹内容。

### Text that morphs

没有文本 morph、交叉淡入或 glyph 插值。文本每帧重建；字符串变化会直接跳变，除非调用方手动淡入或动画颜色。

## When nothing moves

`transition` 与 `value_transition` 第一次直接初始化到目标，新建状态不会动画进入。目标等于当前目标时不重启。Reduce-motion 只有在调用方检查 `Context::reduce_motion` 时才让自定义动画跳过；transition 本身仍按其 animation 时长运行。

`complete()` 把 bounds 或值 transition 吸附到目标，`cancel()` 冻结在当前值。完全不动画可使用 `instant_state()` 或零时长 `Animation`；没有全局“禁用 transition”开关。
