MyRust 每帧发出 `AccessNode` 元数据。它是 `role`、`label`、`value`、`disabled`、`focused`、`bounds` 的扁平 vector；未连接 AT-SPI、UIA 或 NSAccessibility。

```rust
let frame = ui.render(Size::new(640.0, 360.0))?;
for node in &frame.access_nodes {
    println!("{:?} {} {:?}", node.role, node.label, node.value);
}
```

## Names

标准控件从可见文本或固定英文串设置 `label`。`text` 用内容作 label；button 用文本；`text_field` 用 `"text field"`；`checkbox` 用 `"checkbox"`；group 用 `"column"`、`"row"`、`"container"` 或 `"grid"`。

没有公开的 accessible name authoring API、label 关联、description 或 name-from-author。自定义绘制不修改 crate 就无法添加/改变 `AccessNode`。

## Roles

当前发出的 `Role` 包括 `Button`、`CheckBox`、`Slider`、`TextField`、`Group`、`Label`、`ScrollView`、`Switch`、`RadioGroup`、`Radio`、`ComboBox`、`Option`、`TabList`、`Tab`、`ProgressBar`、`Spinner`、`Badge`、`Disclosure`、`Popup`。

没有 `List`、`Table`、`Grid`、`Dialog`、`Alert`、`Landmark` 或 `Link` role。列表暴露 `Option` 行，tabs 暴露 `TabList`/`Tab`，overlay 暴露 `Popup`；其他语义用 `Group`/`Label` 近似。

## States

状态是 `value` 字符串：checkbox `"checked"`/`"unchecked"`，tab `"selected"`/`"unselected"`，switch `"on"`/`"off"`，option `"selected"`/`"unselected"`，back/forward `"enabled"`/`"disabled"`，disclosure 在发出处为 open/closed。`focused` 是布尔。

`AccessNode::disabled` 存在，但 producer 目前硬编码 `false`；标准控件没有公开 disabled API。expanded/collapsed、busy、invalid、required、readonly 与三态 checked 未表达。

## Suggestions

`AccessNode` 没有 suggestion list、active descendant、autocomplete 或 combobox 展开状态。`ComboBox` 与 `Option` 节点可为 popup 发出，但辅助技术看不到关系或高亮项。

## Descriptions and errors

没有 description、error、validation 或 help 字段。form label/hint 只是视觉；`form_field` 的 group/label 不发出这些关系。

## Orientation

slider、scroll view、split view、tab list 都不发 orientation 字段。视觉几何可能暗示水平，但 AT 只收到 bounds。

## Keyboard shortcuts

没有 shortcut 元数据、access key 或 accelerator。button/field 响应按键，但绑定不暴露给辅助技术。

## Formats of text

所有无障碍文本是纯 `String`。`AccessNode` 没有 markup、language tag、pronunciation、selection range 或富文本属性。

## Lists and tables

列表发出 `Role::Option` 行，但没有 list container、行数、选择模式或多选状态。表格为可排序表头发出 button；表头/单元格不是 table/row/column 模型。

## Place in a set

没有 position-in-set 或 set-size 元数据（`posinset`/`setsize`）。列表 `Option` 行与 grid view `Button` 单元只带自身 label/value/bounds。

## Selected, without the look

选中态通过视觉颜色表达，部分控件用 `"selected"`/`"unselected"` 字符串 value。不是每个节点都有专用 selected 布尔；自定义选中视觉没有语义对应。

## Grids

没有 `Grid` role、行列索引或 cell 坐标。`grid` 与 `grid_view` 输出 group/button 和普通 bounds；空间位置只隐含在几何中。

## Unavailable, still reachable

部分控件用 muted 视觉表示禁用，但 `AccessNode::disabled` 永远 false，且无 disabled input API。看起来禁用的控件仍可能接收指针输入，除非调用方不构建它。

没有“disabled but focusable”或“read-only”状态区分。

## Out of the pointer's way

没有 hidden/occluded 标志。bounds 可能在屏外、被 `scroll_view` 裁剪或被 overlay 覆盖，但 `AccessNode` 仍包含；命中测试用独立交互区域与 overlay 层。

## Out of assistive technology's way

没有 `aria-hidden`、accessibility-hidden 或 excluded subtree API。不构建控件会移除节点；构建就总会发出元数据。

## Announcements

没有 live region、alert、status 或 announcement API。toast 与 dialog 有视觉 role，但没有宣告事件。

## Landmarks, live regions and rules

没有 landmark role、live-region flag、atomic/relevant 属性、错误宣告或 accessibility rule。`AccessNode` 是逐帧扁平列表，不是带关系的语义树。

平台 AT 桥需要 role/action 映射、焦点/选择事件、窗口句柄与树 diff；`Inspector` 只用于调试快照，不是 AT。
