动效是调用方持有的标量动画，加上可确定性步进的 spring、keyframe 与 timeline 原语。帧调度器根据动画 deadline 计算下一次唤醒，不再依赖轮询。

```rust
use std::time::Duration;
use myrust_core::ui::{Animation, Easing};

let progress = c.animate_with(
    "panel",
    if open { 1.0 } else { 0.0 },
    Animation::new(Duration::from_millis(180), Easing::EaseOutCubic),
);
```

## Animations

`Animation` 是 `{ duration, easing }`；`Easing` 是 `Linear`、`EaseOutCubic`、`EaseInOutCubic`。`Context::animate(id, target)` 默认 120ms ease-out；`animate_with` 接受显式 `Animation`。

动画按稳定 id 存在 `Context`。目标变化从当前值重定向。标量动画仍没有声明式控件。

## Springs

`Spring` 是独立的标量弹簧，包含 `stiffness`、`damping`、位置、速度与目标。它使用半隐式 Euler 积分，`step(dt)` 完全确定，测试可用固定 dt 驱动。

```rust
use std::time::Duration;
use myrust_core::ui::Spring;

let mut spring = Spring::new(180.0, 20.0).with_target(1.0);
let value = spring.step(Duration::from_millis(16));
spring.set_target(0.0); // 保留当前位置与速度
```

目标变化保留速度，因此被打断的运动能继续带惯性。`cancel()` 停在当前位置，不吸附到目标；`complete()` 吸附到目标并停止。`is_active()` 与 `is_complete()` 分别描述“仍在运动”和“已在目标”。

由 `Context` 管理的 keyed 值可用 `animate_spring(id, target)` 使用默认 spring，或用 `animate_spring_with(id, target, spring)` 指定 stiffness/damping。第一次调用建立静止 target；之后 target 变化会保留 velocity。计时来自 `frame_time`，reduce motion 直接落到 target，收敛后不再请求帧，因此 `next_animation_frame()` 变为 `None`。

stiffness 决定回弹速度，damping 决定振荡衰减；低 damping 容易过冲，高 damping 更平缓。rest threshold 决定何时停止积分。

## Keyframes

`Keyframes` 是按键时间排序的 `{ time, value, easing }` 停靠点。第一个停靠点之前保持首值，最后一个之后保持末值；重复时间戳保留最后提供的值。

```rust
use std::time::Duration;
use myrust_core::ui::{Easing, Keyframe, Keyframes};

let track = Keyframes::new([
    Keyframe::new(Duration::ZERO, 0.0),
    Keyframe::eased(Duration::from_millis(120), 1.0, Easing::EaseOutCubic),
]);
let value = track.value(Duration::from_millis(60));
```

`Keyframes::from_values` 生成等距线性轨道。采样是纯函数，不读取时钟。

## Timelines

`Timeline` 用一个 playhead 驱动任意多个 `Tween`。每个 tween 有 id、起止值、可选 delay、duration 与 easing。`advance(dt)` 返回 `TimelineFrame`，其中 `values` 按插入顺序排列，并带有 iteration、progress 与完成状态。

```rust
use std::time::Duration;
use myrust_core::ui::{Timeline, Tween};

let mut timeline = Timeline::new()
    .tween("opacity", 0.0, 1.0, Duration::from_millis(180))
    .add_tween(
        Tween::new("offset", 0.0, 24.0, Duration::from_millis(220))
            .with_delay(Duration::from_millis(40)),
    );
let frame = timeline.advance(Duration::from_millis(16));
for value in frame.values {
    let _ = (value.id, value.value);
}
```

新建 timeline 立即播放。`pause()` 暂停但保留 playhead；`cancel()` 同样停止且不触发完成；`seek()` 只移动 playhead，不触发回调；`complete()` 跳到末帧。`on_complete` 只在真正完成时触发一次；`restart()` 重置状态。

`repeat(n)` 表示整条时间线播放 n 次；`repeat_forever()` 无限循环，直到取消或显式完成。每个 cycle 都会重新计算 tween delay。

## Poses that only paint

仍然没有 pose 或 paint-only 动画对象。调用方可存储颜色、圆角、类似透明度的值或 draw-command 位置并每帧重建命令；布局不会自动排除。

## Delays and staggering

Timeline 级 delay 与 per-tween delay 可覆盖一次性延迟和错峰。独立状态机驱动的值仍可分别使用 spring 或标量动画。

## Gestures

没有 gesture recognizer、指针速度跟踪、drag/scroll physics，也没有内置的可打断手势 spring。`Spring` 提供物理原语，但控件仍自己处理拖动；自定义手势需从 `UiEvent` 指针值与应用状态构建。

### By hand

自定义手势可跟踪 `Context::pointer_position()` / `pointer_down()` 与上一帧区域，或通过 `NativeUi` 接收原始 `UiEvent`。仍然没有 gesture arena 或 pinch recognizer。

## When it is done

`animate_with` 返回当前标量，不是完成标志。`Spring` 用 `is_active()` / `is_complete()` 报告状态；`TimelineFrame` 带 `completed`，timeline 还可安装 `on_complete`。bounds transition 继续用 `TransitionFrame::done`。

## Without animation

用 `Duration::ZERO` + `Easing::Linear`，或直接赋值；`instant_state()` 对 `TransitionState` 做同样的事。`Context::reduce_motion` 报告环境标志，调用方可自行跳变或直接完成 spring。

## Scheduling frames

`FrameScheduler` 是逐帧的 deadline 集合。`request_frame(now)` 按调度 cadence（约 60Hz）请求下一帧，`request_frame_now(now)` 表示立即到期，`request_at` / `request_after` 提供显式 deadline。空闲时 `next_wake(now)` 返回 `None`。

```rust
use std::time::Duration;
use myrust_core::ui::FrameScheduler;

let mut scheduler = FrameScheduler::new(Duration::from_millis(16));
scheduler.request_frame(now);
if let Some(deadline) = scheduler.next_wake(now) {
    // 宿主：ControlFlow::WaitUntil(deadline)
}
```

`AnimationDriver` 把调度器与显式时钟组合，供确定性宿主与测试使用。`Context::next_animation_frame()` 与 `NativeUi::next_animation_frame()` 暴露同一 deadline。native app loop 用 tao 的 `ControlFlow::WaitUntil` 等待，只在到期时请求重绘；空闲窗口不轮询。

活跃标量动画与 springs 都会贡献 deadline。`Timeline` 由调用方 `advance(dt)` 推进；返回 frame 未完成时，调用方应 `request_animation_frame()`。所有 spring/tween 收敛或取消后 scheduler 进入 idle，不再留下 timer wake。

## Scrolling

没有滚动联动或滚动位置动画 API。`scroll_view` 通过 `ScrollState` 报告 offset；`Context::wheel_delta()` 暴露滚轮输入给自定义效果。Parallax/sticky 需调用方构建。

## Content that changes

内容或布局变化没有隐式过渡。新尺寸下重建视图会跳变，除非调用方动画矩形/值或使用 `Timeline`。没有 diffing 或布局动画缓存。

## Loops and your own motion

`spinner` 是内置循环，按调度 cadence 请求帧。`Context::request_animation_frame()` 让自定义视图请求下一帧；app/host 在计算出的 deadline 唤醒。`ViewFrame::needs_animation_frame` 仍保留。

## Less motion and frames

`MYRUST_REDUCE_MOTION` 设置 `Context::reduce_motion`；内置 spinner/transition 在 true 时跳过或直接落定。没有 OS 实时偏好回调，也没有 FPS/vsync API。

`needs_animation_frame` 为 false 时，`next_animation_frame()` 返回 `None`。隐藏窗口被排除在 native 唤醒计算之外，重新可见前不会继续动画。

## Out of sight

native app loop 在 `is_visible()` 为 false 时不安排帧。`scroll_view` 对列表只构建可见行，但可见窗口内的任意动画只要调用方持续更新就会继续。

要显式停止工作，调用方跳过 `animate_with`/`request_animation_frame`、取消 spring 或 timeline，或丢弃动画控件。没有 intersection observer 或 occlusion API。
