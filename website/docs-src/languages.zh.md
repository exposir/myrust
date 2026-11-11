语言支持刻意保持最小。`myrust-ui` 只为 `en-US` 与 `zh-CN` 提供格式化；没有 catalog、消息查找、复数规则或 UI 语言状态。

```rust
use myrust_core::ui::{format_date, format_number, DateStyle, Locale};

let n = format_number(1234567.89, Locale::EnUs, 2)?;
let d = format_date("2024-02-29", Locale::ZhCn, DateStyle::Short)?;
```

## Catalogs

没有 catalog 类型、loader、domain 或 fallback-chain API。字符串是调用方选择的普通 Rust 值；需要时应用可自己维护 `HashMap<Locale, ...>`。

## Messages

没有 message formatter、占位符插值、plural/select 或 number skeleton API。替代是 `format!` 与调用方字符串；占位符和复数选择必须由应用实现。

## In Go

参考页使用 Go 的 catalog 包；MyRust 没有 Go catalog 集成，也没有等价包。此小节为结构保留，功能是缺失而非移植。

## The language the app shows

`Locale` 只影响数字/日期格式化。它不设置 `Context` 语言、内置控件文案、校验文本或无障碍 label；`Context` 没有 current-language 字段或 locale setter。

### When the language changes

改变 `Locale` 只是改变调用方值；下一次 view build 使用它。没有语言变化事件、catalog 重载或额外自动重渲染，只有下一次 `NativeUi::render`。

### Native UI

内置控件使用固定英文串，如 `"text field"`、`"checkbox"`、`"back"`。它们不本地化，也没有逐控件覆盖。调用方可用翻译后的 `text` 包裹控件，但部分 role 的 AccessNode label 仍是英文。

### Regional formats

只有 `Locale::EnUs` 与 `Locale::ZhCn`。`format_number` 两者都用逗号分组与点小数；日期短格式不同（`MM/DD/YYYY` 对 `YYYY年M月D日`）。ISO 输出与 locale 无关。

## Formatting numbers and dates

`format_number(value, locale, decimals)` 返回 `Result<String, FormatError>`；拒绝非有限值与超过 18 位小数。`format_date` 只接受 `YYYY-MM-DD`，校验闰年，返回 ISO 或所选 locale 的短格式。

### Relative times

没有相对时间格式化（“2 小时前”“下周”），也没有时区/日历转换 API。日期只是日历字段；时间由 `time_input` 单独处理。

### Formatting data

没有 locale-aware list、currency、percent、measurement 或 unit 格式化。`format_number` 是唯一数字格式化器；日期用 `format_date`。其他由调用方负责。

## MyGo's own strings

MyGo 有框架自有翻译字符串；MyRust 没有。内置 UI 文案是英文字面量，站点自身的双语内容在 `website/docs-src` 单独生成。

没有框架 chrome 的运行时 string table，因此需要翻译控件文案的应用必须自绘 label，或用带 label 的 group 包裹控件。

## Which message

没有 plural category、消息选择、gender 或 context-aware lookup。调用方为当前 locale 和值选择完整字符串。

## Checking the catalogs

`build.py` 检查两种文档语言；UI crate 没有 catalog 完整性检查。缺失翻译是调用方问题，`format_number`/`format_date` 只对非法值/日期返回错误。

## Trying a language

Languages demo 在 `Locale::EnUs` 与 `Locale::ZhCn` 间切换并格式化同一数字/日期。UI 语言不变，只改变格式化字符串。

```rust
let locale = if app.zh { Locale::ZhCn } else { Locale::EnUs };
text(c, &format_number(1234567.89, locale, 2)?);
text(c, &format_date("2024-02-29", locale, DateStyle::Short)?);
```

## In pages

文档站有 `website/docs-src/build.py` 生成的 en/zh 页面；这与 `myrust-ui` 分离。Web 页面可用自己的 i18n 资产，native UI 只有 `Locale` 格式化。

## Package i18n

### Adding a language to MyGo

没有可扩展的 locale-data 包。添加语言意味着在 `languages.rs` 增加 `Locale` variant 与显式格式规则；基于 catalog 的 MyGo 扩展没有等价物。

除 `languages.rs` 及其 `Locale`、`DateStyle`、`FormatError`、`format_number`、`format_date` 外没有包级 i18n 模块。添加语言意味着在代码中添加 variant 与显式格式规则，而不是安装 locale 数据包。
