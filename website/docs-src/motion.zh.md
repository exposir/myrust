动画是调用方持有的标量动画加帧时钟。`Context::animate` / `animate_with` 把值推进到目标；`spinner` 与 `transition` 是内置使用者。

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

动画按稳定 id 存在 `Context`。目标变化从当前值重定向。没有声明式 `Animation` 控件，也没有完成回调。

## Poses that only paint

没有 pose/paint-only 动画对象。调用方可存储标量并每帧重建颜色、圆角、类似透明度的混合或 draw-command 位置；布局不会自动排除。

## Delays and staggering

没有 delay、stagger、sequence 或 orchestration API。错峰需调用方在不同时间启动独立动画或偏移目标；`Animation` 只有 duration/easing。

## Gestures

没有 gesture recognizer、velocity tracking、drag/scroll physics 或可打断 spring。控件自己处理指针拖动；自定义手势需从 `UiEvent` 指针值与应用状态构建。

### By hand

自定义手势可跟踪 `Context::pointer_position()` / `pointer_down()` 与上一帧区域，或通过 `NativeUi` 接收原始 `UiEvent`。没有 gesture arena、pinch recognizer 或 inertia helper。

## When it is done

`animate_with` 返回当前标量，不是完成标志。可与目标比较，或对 bounds transition 用 `TransitionFrame::done`。没有 `on_complete` 回调。

## Without animation

用 `Duration::ZERO` + `Easing::Linear`，或直接赋值；`instant_state()` 对 `TransitionState` 做同样的事。`Context::reduce_motion` 报告环境标志，调用方可自行跳变。

## Keyframes

没有 keyframe、tween-sequence、多停靠点或 path 动画 API。调用方可在应用状态里按时间链式切换目标；每一步仍是一个标量/transition。

## Timelines

没有 timeline 对象、播放控制、time remapping 或 group。`TransitionState` 是单个动画矩形；多个值需分开状态或 `animate_with` 调用。

### Labels and stagger

没有 label、marker、stagger group 或 timeline callback。错峰由应用调度。

### Playing it

没有 play/pause/seek/rate API。动画时间是帧时钟；需要暂停的调用方必须冻结自己的 elapsed time。

## Scrolling

没有滚动联动或滚动位置动画 API。`scroll_view` 通过 `ScrollState` 报告 offset；`Context::wheel_delta()` 暴露滚轮输入给自定义效果。Parallax/sticky 需调用方构建。

## Content that changes

内容或布局变化没有隐式过渡。新尺寸下重建视图会跳变，除非调用方动画矩形/值。没有 diffing 或布局动画缓存。

## Loops and your own motion

`spinner` 是内置循环并请求 animation frame。`Context::request_animation_frame()` 让自定义视图请求下一帧；app/host 重新渲染。`ViewFrame::needs_animation_frame` 暴露该状态。

没有 loop manager、frame budget、tween scheduler 或 retained animation graph。

## Less motion and frames

`MYRUST_REDUCE_MOTION` 设置 `Context::reduce_motion`；内置 spinner/transition 在 true 时跳过或直接落定。没有 OS 实时偏好回调，也没有 FPS/vsync API。

`needs_animation_frame` 是 best-effort。native window loop 决定何时唤醒；隐藏窗口或慢宿主可能停止帧，动画时间不能假设固定帧率。

## Out of sight

没有可见性跟踪或离屏自动暂停。`scroll_view` 对列表只构建可见行，但任意动画只要调用方持续更新就会继续。

要停止工作，调用方跳过 `animate_with`/`request_animation_frame` 或丢弃动画控件。没有 intersection observer 或 occlusion API。
