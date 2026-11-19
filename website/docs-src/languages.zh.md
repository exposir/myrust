`myrust-ui::i18n` 现在提供 locale 匹配、fallback 查找、ICU MessageFormat 渲染，以及基于 CLDR 的数字和日期格式。CLDR 数据通过 ICU4X 编译进二进制；运行时不会下载 locale 数据包。

```rust
use myrust_ui::{I18n, LocaleId, MessageArguments, MessageCatalog, MessageValue};

let mut i18n = I18n::from_system([
    LocaleId::try_new("en-US")?,
    LocaleId::try_new("zh-CN")?,
]);
i18n.add_catalog(MessageCatalog::try_new(
    LocaleId::try_new("zh-CN")?,
    [
        ("file-count", "{count, plural, other {# 个文件}}"),
        ("greeting", "你好，{name}"),
    ],
)?)?;

let text = i18n.translate(
    "greeting",
    &MessageArguments::new().with("name", MessageValue::Text("Ada")),
)?;
println!("{}", text.text); // "你好，Ada"
```

## Locale 解析

原生平台上的 `system_locales()` 读取操作系统偏好的 locale 列表。`resolve_system_locales()` 按顺序遍历该列表：先匹配精确 locale，再匹配同语言、script 与 region 的相关变体，最后回退到英语。默认值是 `en-US`；如果应用提供另一种英语 catalog，它会成为 fallback 链的英语终点。

`LocaleId` 负责校验并规范化 BCP 47 标识符。即使调用方的支持列表只有非英语 locale，`I18n` 也会补上英语 fallback。

`en-XA` 是布局测试 locale。把它加入支持列表并调用 `set_locale("en-XA")` 后，消息输出会加上重音并扩展到源文本长度的至少 150%。基础文本仍来自英语 catalog。

## Catalog 与 fallback

`MessageCatalog::try_new(locale, [(key, message), ...])` 会在接受 catalog 前解析全部消息。语法错误包含 key、原始消息和字节位置；重复 key 会被拒绝。

`I18n::translate` 依次尝试当前 fallback 链中的 locale。即使消息来自 fallback catalog，也能正常渲染，但 `Translation` 会记录 `used_locale`、`fallback_from` 与 `pseudo_localized`，供调用方记录或展示 fallback。所有 catalog 都缺少 key 时，返回 `I18nError::MissingKey`，并列出已经尝试的 locale。

Catalog 是作者维护的数据，不是编程语言字符串的替代品。缺翻译不会被静默编造：要么由 fallback catalog 提供，要么返回错误。

## 消息语法

当前实现了 ICU MessageFormat 的一个子集：

- 插值：`{name}`
- plural：`{count, plural, =0 {...} one {...} other {...}}`，支持 `#`、`offset:N`、精确选择器与 CLDR 复数类别
- select：`{kind, select, a {...} b {...} other {...}}`
- number 参数：`{amount, number}`、`{amount, number, integer}`
- date 参数：`{day, date}`、`{day, date, short}`、`{day, date, iso}`
- 字面量引号：`''` 与用单引号包住的整段文本

复数类别由 `icu_plurals` 选择，而不是硬编码 `one`/`other`。俄语、波兰语、阿拉伯语等语言会因此得到正确分支。`plural` 与 `select` 都必须包含 `other` 分支。

参数值使用 `MessageValue::Text`、`MessageValue::Number` 或 `MessageValue::Date`。缺少参数返回 `MissingArgument`；把数字传给 `select`，或把文本传给 `plural`，返回 `WrongArgumentType`。非法 ISO 日期和非有限数字会返回错误，而不是输出空串。

## 语言切换

`I18n::set_locale`、`set_system_locales` 和 `refresh_system_locale` 返回 `LanguageChange`。`changed` 表示选中的 locale 是否真的改变，`generation` 是每个实例单调递增的计数。视图可以保留上次渲染时的 generation，在变化时重建自己的翻译文案。

这是 API 层的接入点。`Context` 与内置控件尚未读取一个全局 `I18n` 值，因此框架自有文案（例如 `"text field"`、`"checkbox"`）目前仍是固定英文，直到控件接受可翻译 label。

## 数字与日期格式

兼容函数 `format_number(value, locale, decimals)` 与 `format_date(date, locale, style)` 现在委托给 ICU4X。`en-US` 与 `zh-CN` 的既有输出保持不变，包括美国短日期 `M/D/YYYY` 与中文短日期的本地化年月日标记；其他 locale 可通过消息使用自己的 CLDR 格式。

```rust
let n = format_number(1234567.89, Locale::EnUs, 2)?; // "1,234,567.89"
let d = format_date("2024-02-29", Locale::ZhCn, DateStyle::Short)?;
```

分组符、小数符、负号与日期顺序来自 CLDR。实现的 decimal/date 子集之外的格式仍需要单独 API。

## 限制

没有 currency、percent、measurement、unit、relative-time 或 time-zone formatter。消息解析实现了上述子集，不是完整的 MessageFormat 2 语法。内置 UI chrome 不会自动翻译，改变 `I18n` 也不会自行重绘视图；应用需要根据 generation 重建界面。

## In Go

参考实现从 Go 的 i18n 包获得 catalog 与格式化数据。MyRust 直接使用等价的 ICU4X 组件，数据在编译期嵌入，不需要运行时数据服务。

## Package i18n

包级 API 位于 `myrust-ui::i18n`：`LocaleId`、`I18n`、`MessageCatalog`、`MessageArguments`、`MessageValue`、`Translation`、`I18nError`、locale resolver、`system_locales` 与 `pseudo_localize`。`myrust-ui::languages` 继续作为 `Locale`、`DateStyle`、`format_number` 与 `format_date` 的稳定兼容入口。
