迁移是显式、基于 id 的。`Context::migrate_state(old, new)` 移动 Context 持有的瞬态控件状态；`migrate_value(map, old, new)` 移动调用方 map 的一项。没有 source rewrite 或 runtime 兼容层。

```rust
if renamed {
    c.migrate_state("editor-old", "editor-new");
}
```

## Apply the source migration

没有自动 source migration、codemod、版本升级脚本或 deprecated API shim。重命名控件/字段是手动 Rust 修改；能保持 id 稳定就保持。

## Element values and keys

`migrate_value` 在新 key 不存在时移动一个 `HashMap<String, T>` 项。`Context::migrate_state` 移动 `widget_states`（文本 cursor/selection、select open/highlight、scroll offset、slider drag）、以 id 为前缀的 animation key、overlay runtime 与 focus。

两者都不覆盖已存在的新 key；相同 id、缺失 old 返回 false。序列化应用值不会自动迁移。

## Input and actions

输入事件不迁移。`UiEvent` 指向当前帧矩形；id/结构变化后调用方需保持相同交互 id，否则 pending input 会丢失。

没有 command map、keybinding migration 或 action-id registry。action 是视图中的普通 Rust 调用。

## Persistent identity

持久 identity 是 Context 状态与焦点的稳定字符串 id，加上调用方持有的状态结构。没有 element handle generation 或 identity 对象可迁移。

控件移到新 parent 时保持 id；id 本身变化时调用 `migrate_state`。交互区域每帧重建，只有状态/焦点需要显式移动。

## Focus bound to app data

焦点是字符串 id，不绑定数据。旧 id 聚焦时 `migrate_state` 移动 `focused_id`；数据集合重排需稳定行 id 或显式 `request_focus`。

没有 `Focus<T>` 迁移或数据键映射 helper。

## Lists and keyboard commands

列表状态含 selection；`ListReorderState` 有 drag/from/to，列表行用 Option 节点。除稳定行 id 外没有列表 identity 迁移，也没有 keyboard-command 表可迁移。

`migrate_state` 可按 list id 移动 scroll/selection 类控件状态，但行 id 与数据顺序仍由调用方负责。

## Persistent services and verification

没有 service registry、callback handle 或后台任务迁移。callback 在调用方存活，必须重建；`migrate_state` 只处理 UI context 状态。

用 `cargo test` 与对 `ViewFrame`/`AccessNode` 的显式断言验证。`migrate_value`/`migrate_state` 在 old 缺失或 new 已存在时返回 false，且不合并部分状态。
