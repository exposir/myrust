`dialog(c, "id", &mut open, width, content)` 在 scrim 上绘制居中 modal panel。content 闭包返回 `DialogAction::{None,Confirm,Cancel}`。

```rust
use myrust_core::ui::{dialog, DialogAction};

let response = dialog(c, "rename", &mut app.open, 320.0, |c| {
    if button(c, "Save").clicked { DialogAction::Confirm } else { DialogAction::None }
});
if response.confirmed {
    app.commit();
}
```

## The keyboard

打开时聚焦 panel 内第一个可聚焦控件；Tab/Shift+Tab 被限制在其中，Escape 关闭，焦点恢复到 opener。外部指针输入被 scrim 阻止。

## Keeping it open

`open` 由调用方持有。返回 `None` 保持打开；`Confirm`/`Cancel` 关闭并报告 `confirmed`/`cancelled`。没有异步校验 hook 或 “prevent close” callback。

## Beyond the window

dialog 是窗口内 overlay，不是 OS modal/sheet。不能超出窗口、创建独立 native alert panel 或阻塞其他应用。

## Without a look

没有底层 modal builder、自定义 width/animation 或独立 title bar。`dialog` 拥有 scrim/panel；自定义 chrome 需 `overlay` + 调用方内容。

## Accessibility

panel 发出 `Role::Popup`，label 为 dialog id；title/body 是 `Role::Label`，按钮是 `Role::Button`。没有 Dialog/AlertDialog role、modal flag、焦点限制元数据或 AT 桥。
