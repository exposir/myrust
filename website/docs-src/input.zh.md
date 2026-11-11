输入通过 `NativeUi::input(UiEvent)` 进入视图。一帧看到上一帧以来累积的指针与按键；控件使用上一帧交互 bounds，因此点击在指针于同一区域释放后报告。

```rust
ui.input(UiEvent::PointerMoved { x, y })?;
ui.input(UiEvent::Key { key: Key::Tab })?;
let frame = ui.render(Size::new(640.0, 360.0))?;
```

## The pointer

`Context::pointer_position()` 返回最新逻辑点，`pointer_down()` 报告主键，`wheel_delta()` 返回累积滚轮移动。标准控件用上一帧注册 hover/press/click 区域。

没有 pointer capture、拖动阈值、cursor-shape API、右键查询或多点触控模型。`UiEvent::PointerPressed/Released` 携带主键；自绘指针控件需把这些事件与自己的上一帧 bounds 结合。

### A press the system cancels

没有区别于 `PointerReleased` 的取消事件。离开区域的按压不会点击；控件把 release 与上一帧区域比较。OS 手势取消未表达，需要取消语义时由调用方自行合成状态。

## The keyboard focus

Tab/Shift+Tab 按构造顺序遍历可聚焦控件；overlay 在其顶层限制焦点。`Context::focus(id)` 查询归属，`request_focus(id)` 移动焦点。

可聚焦控件绘制焦点环，并在适当处处理 Enter/Space。没有公开 focus-order override、roving tab index 或 focus-scope 对象；顺序由构造顺序和 overlay layer 决定。

## Persistent focus

焦点是 `Context` 中的稳定字符串 id，只要控件仍可聚焦就跨帧保留。移除控件或改 id 会丢失焦点；`migrate_state` 可在重命名时移动焦点。

没有逐窗口 focus handle，也没有任意外部窗口后的焦点恢复 API。调用方可在下一帧调用 `request_focus`。

## Focus bound to data

焦点不绑定数据模型或集合索引，而是调用方稳定 id；重排数据的列表必须保持行 id 稳定，否则焦点会跟随旧 id。

没有 `Focus<T>` 包装或按数据键的焦点注册表。调用方把数据映射为 id，结构性变更后可调用 `request_focus`。

## Focus groups

没有公开 focus-group 容器或组级方向键导航。组就是构造顺序；overlay 形成内部顶层焦点陷阱。

tabs 与 segmented 内部的方向键切换由控件实现，不是通用 focus-group 机制。嵌套焦点组和 escape 到父组不可用。

## Shortcuts

没有全局快捷键注册表、修饰键匹配或命令表。按键作为 `UiEvent::Key` 到达；控件消费自己的键，应用可在 `NativeUi` 前后检查自己的事件。

`Context` 没有公开 key-pressed 查询，因此应用快捷键应在视图外处理，或由自定义可聚焦控件处理。修饰键状态与快捷键优先级未建模。

## Keys held

`UiEvent::Key` 是离散按下，不是 down/up 生命周期。没有 held-key 查询、自动重复区分或 key-release 事件。

当前 context API 无法观察按住键、组合键或修饰键和弦。若平台发送原始事件，应用仍可在 `NativeUi` 外维护自己的 pressed 集合。

## Input methods

`UiEvent::TextInput { text }` 把完成字符串插入聚焦文本字段。没有 preedit、组合范围、候选窗或 IME commit/cancel 事件。

文本字段支持光标与选区，但 IME composition 明确不在范围内；宿主 IME 只能投递已提交文本。Bidi 输入与复杂文字 shaping 未实现。

## Dropped files

原生界面窗口不接收操作系统文件投放。文件投放属于 WebView 窗口（`myrust-core` 的 `FileDropEvent` / `FileDropHandler`），不属于 immediate-mode native view。

没有 `NativeUi` drop 回调。调用方可实现自己的平台文件投放桥并写入应用状态，但那在本 crate 之外。

## Scrolling and pinching

`scroll_view(c, id, viewport_height, content)` 裁剪内容并处理滚轮、滚动条拖动与键盘滚动。`Context::wheel_delta()` 为自定义控件暴露累积滚轮输入。

pinch/zoom、惯性滚动、overscroll 与双轴滚动容器未实现。scroll view 是垂直的；水平滚动与触摸手势缺失。

## Every key, as it comes

`NativeUi::input(UiEvent::Key { key })` 传递命名按键；`UiEvent::TextInput` 单独传递已提交文本。`Key` 包含 Tab/ShiftTab、Enter、Space、Backspace、Delete、方向键、Home、End、PageUp、PageDown。

native 窗口 translator 只映射一部分：Tab、Enter、Space、Backspace、Delete、Left/Right、Home、End。PageUp/PageDown 存在于 `Key`，但 `app/mod.rs` 尚未映射，native 窗口在补映射前无法产生它们。

### Every key of the keyboard

没有原始 scancode、物理键、修饰键 bitmask 或带修饰键的文本模型。字符键在平台布局后作为文本送达；命名导航键使用有限的 `Key` 枚举。

功能键、小键盘区分、dead key 与 composition 数据未表达。自定义宿主可直接构造 `UiEvent`，但内置 native adapter 不暴露完整键盘。

## Input inside a region

命中测试使用上一帧交互区域。`scroll_view` 与 `split_view` 为活动偏移调整指针坐标；`Context::transform` 记录仿射变换并把指针逆变换到局部 bounds。

普通 scissor clip 不改变命中区域，任意 `ScissorPush` 命令也不会创建坐标空间。自定义裁剪内容必须让交互 bounds 与布局处于同一局部空间。

## See also

- Layout：流与裁剪。
- Text：文本测量与行内限制。
- Styling and themes：焦点环与颜色。
- Accessibility：发出的节点；未连接平台 AT 桥。
