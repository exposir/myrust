inspector 是逐帧快照，不是 live devtools protocol。`inspect_frame(&ViewFrame)` 构建 `InspectorTree`；`inspector(c, id, &tree)` 将其渲染为 id、role、bounds 与 paint 摘要的缩进列表。

```rust
use myrust_core::ui::{inspect_frame, inspector};

let tree = inspect_frame(&frame);
inspector(c, "debug-tree", &tree);
```

## Elements

每个 `InspectorItem` 有 `id`、`role`、`label`、`bounds`、`paint` 与嵌套 `children`。嵌套由 bounds 包含关系推断，不是 renderer 持有的 parent 指针；重叠或相同 bounds 可能得到近似树。

`PaintSummary` 统计与 item bounds 相交的 rect、rounded rect、circle、line、text、SVG path、gradient、image 命令。父节点会重复计算子节点命令；这是诊断计数，不是分配数据。

## Changing styles

没有 live style editor 或 mutation API。树是上一帧派生的不可变数据；改变外观需修改调用方状态或 `Theme` 后重新渲染。

## Forcing states

不能通过 inspector 强制 hover、focus、disabled 或 selected。测试可注入 `UiEvent` 并调用 `Context::request_focus`；没有 `force_hover`/`force_active` 标志。

## Showing layouts

列表显示 access node 的绝对 `bounds`。不绘制 overlay box、padding、margin、flex line 或 clip region；更深入检查请用 `ViewFrame.layout`/`display` 数据。

## Opening the source

没有 source file/line 链接。`id` 在 bounds 匹配交互时使用稳定控件 id，否则由 role/label 派生；label 与 paint 摘要是唯一 provenance。

## Performance and issues

`inspect_frame` 遍历帧的 access nodes 与命令；分配树并 clone 标签，是调试路径，不是帧耗时工具。`FrameStats` 单独报告生产时间；inspector 不 profile 控件。

重叠 bounds、重复 label、transform/内容裁剪会让树有误导。没有 live 更新、选择、搜索、展开/折叠或 issue 标注。
