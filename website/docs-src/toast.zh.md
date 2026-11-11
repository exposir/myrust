`toasts(c, "id", &mut queue)` 在右上角绘制调用方 `ToastQueue`。`Toast::new(message)` 与 `.action(label)` 构建条目；`push` 接收 `&Context` 启动时钟。

```rust
let mut toast = Toast::new("Saved");
queue.push(c, toast);
let response = toasts(c, "toasts", &mut queue);
```

## Adding and closing toasts

`ToastQueue::push`/`push_message` 追加；`toasts` 按 `duration` 过期，并返回 `ToastEvent { index, kind, action }`（timeout/close/action）。close `x` 与可选 action 是 Button。

没有 stacking limit、deduplication、queue reorder，或返回 event 之外的按 id 手动移除。

## In the background

toast 只在 `toasts` 被帧调用时过期；没有后台 timer。view 停止渲染时队列保留。时间使用 `Context::frame_time`。

## Without a look

没有底层 toast builder、自定义 placement、icon 或富 body。`toasts` 拥有圆角 panel 与右上 stacking；任意内容需 custom overlay（但不参与队列过期）。

## Accessibility

toast panel 发出 `Role::Group`，label 为 `"toast"`，value 为 message。message 是 `Role::Label`；action/close 是 `Role::Button`，label 为其文本。

没有 LiveRegion/Alert role、announcement event、timeout 元数据或 AT 桥。screen reader 不会收到通知。
