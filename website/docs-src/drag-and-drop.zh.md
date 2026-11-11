拖放是窗口内交互，不是操作系统会话。调用方持有 `DragState`（`payload`、`origin`、`current`）；source 绘制 ghost，target 报告完成的 drop。

```rust
use myrust_core::ui::{drag_source, drop_target, DragState};

drag_source(c, "card", "Drag me", "card-42", &mut app.drag);
if let Some(drop) = drop_target(c, "bin", "Drop here", &mut app.drag).dropped {
    app.items.retain(|item| item != &drop.payload);
}
```

payload 是 `String`。没有类型化 payload、拖动柄、hover-delay 或拖动时自动滚动。

## Reordering

`reorder_list` 渲染可拖动行并报告 source/target 索引；`reorder` 对调用方数据应用移动。`ListReorderState` 持有 `DragState` 与 `from`/`to`。

```rust
let result = reorder_list(c, "rows", &mut app.reorder, &rows, 26.0);
if result.dropped {
    if let (Some(from), Some(to)) = (result.from, result.to) {
        reorder(&mut app.rows, from, to);
    }
}
```

这个 helper 的列表不是虚拟化列表，不绘制行 ghost，只报告索引；数据顺序仍由调用方负责。

## Between windows and applications

原生界面不发起也不接收 OS 拖放会话。`DragState` 永不离开进程；没有原生 pasteboard、drop target 注册或跨窗口拖动。

WebView 窗口可接收文件投放（`myrust-core` 的 `FileDropEvent`/`FileDropHandler`），但那是独立路径，不等同 native UI 拖放。

### Representations and lazy providers

没有 representation 协商或惰性 item provider。一次拖动只携带调用方预先构造的一个 `String` payload。

更丰富的值可在 payload 中放 id，再从应用状态查找。没有 MIME type、file promise、拖动中缩略图或远程获取。

### Copy, move, completion and cancellation

target 只在释放时报告 `DragDrop { target, origin, payload }`。修饰键驱动的 copy/move、drop effect、完成握手与取消未建模。

在 target 外释放后，状态在下一空闲帧清除；没有“drag cancelled”回调。调用方可比较 origin/target 决定 copy 或 move。

### Native format mapping

没有映射原生拖放格式。Native UI 不能导出 OS 文件、URI、图片或文本选择；窗口内只有 `String` payload。

需要 OS 互操作时，使用 WebView 表面、宿主平台剪贴板 API，或 `myrust-ui` 之外的应用专用桥。
