视图是一个从调用方状态到控件的 Rust 闭包。`NativeUi::new` 保存这个闭包，每帧重新运行；`Context` 每帧重建，而状态在闭包外存活。

```rust
use myrust_core::ui::{button, column, text, NativeUi};

let ui = NativeUi::new(|c| {
    let mut count = 0;
    column(c).padding(16.0).gap(8.0).children(|c| {
        if button(c, "Increment").clicked {
            count += 1;
        }
        text(c, &count.to_string());
    });
});
```

上面的 `count` 故意不是应用状态：闭包每帧重新运行，计数必须放在调用方，通常是 `Rc<RefCell<_>>`。`button` 在指针于上一帧区域释放后报告点击。

## Events and actions

控件在构造时返回输入：`ButtonState::clicked`、`TextFieldState::changed`、`CheckboxState::changed`。动作应在控件之后立即执行，以看到先前控件的编辑。

```rust
if button(c, "Save").clicked {
    save(&app);
}
```

没有延迟 `OnClick` 队列、禁用态捷径或 action 对象。需要异步执行的动作由调用方在视图外调度。

## Keys

控件在相关处消费 `Key::Tab`、`ShiftTab`、`Enter`、`Space`、`Backspace`、`Delete`、方向键、Home/End。宿通过 `NativeUi::input(UiEvent::Key { key })` 投递按键。

`Context` 没有公开的“某键是否按下”查询，供任意自绘控件使用。自定义行为应复用现有可聚焦控件，或由应用在输入进入 `NativeUi` 前处理；修饰键、按键重复和 key-up 未建模。

## Change the state from other goroutines

视图在调用 `NativeUi::render`（或窗口循环）的线程运行。`Context`、`NativeUi` 和捕获状态基于 `Rc`/`RefCell`，不是 `Send`；不能从其他线程修改视图。

没有跨线程 channel 排空或线程安全状态 API。跨线程工作必须在 UI 外完成，再在下一帧前由应用事件循环应用到 UI 线程。把 `Mutex` 共享进视图不符合当前闭包类型。

## State of an element

持久状态由调用方持有：`TabsState`、`TextFieldState`、`ListState`、`CalendarState` 等每帧返回，稳定字符串 id 作为 Context 瞬态状态的键。没有 `Element` 句柄、owner slot 或 generation 检查。

id 必须唯一，不能在不相关控件间移动。`Context::focus(id)` / `request_focus(id)` 按 id 查询与改变焦点；`Context::migrate_state(old, new)` 在重命名控件时迁移瞬态状态。

## Check build lifetimes

Rust borrow checker 在构造期检查访问；没有运行时元素生命周期诊断。控件 builder 可变借用 `Context` 贯穿 builder 表达式和 children 闭包，因此跨另一个 `Context` 调用持有 builder 无法编译。

Context 不保留 builder 到下一帧。陈旧交互靠稳定 id 和 Context 持有的上一帧区域避免，而不是 generation 计数器。测试里可用 `NativeUi::render(size)` 无窗口运行闭包并检查返回的 `ViewFrame`。
