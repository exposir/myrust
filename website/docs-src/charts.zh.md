`bar_chart(c, "id", values, height, min, max)` 与 `line_chart(...)` 从 `&[f32]` 绘制固定高度图表。`ChartResponse` 报告 plot rect、有效 min/max、点数与 kind。

```rust
let values = [0.2, 0.8, 0.4, 0.9];
bar_chart(c, "bars", &values, 120.0, Some(0.0), Some(1.0));
```

## Scales

可传 min/max；否则使用有限数据范围。非有限点被过滤。scale 是线性；没有 log/time/band scale 或 tick 生成。

### Wide ranges and signed data

负/正值在线性 min/max 内可用；没有 zero-baseline 策略、log scale、nice ticks 或数据极值之外的自动 padding。

## The plot and its axes

`ChartResponse::plot` 是 chart-local plot 矩形。不绘制 axis、tick、label、grid line 或 legend；调用方可据 response 添加 text/line 命令。

## Marks

bar 每点一个 rect；line 画连接段。没有 point marker、hover target、tooltip 或逐 mark 颜色。

## Lines and areas

只画 polyline；没有 area fill、curve interpolation 或 gap policy。缺失/非有限点被丢弃，不产生断点。

### Live data

调用方每帧重建 values。没有 streaming buffer、数据集间动画或 auto-scroll。

## Stacks

没有 stacked/grouped bar 或多 series。可画多个 chart 或自定义 rect。

## Pies and donuts

没有 pie/donut/radial chart。自定义 `DrawCommand::SvgPath` 或 circle 可近似。

## Selection

没有 hover/click/brush selection、cursor readout 或 selected index。可在外部加 pointer region/overlay。

## Accessibility

chart root 发出 `Role::Group`，label 为 chart id。没有 per-mark/axis/data-summary AccessNode、chart role 或 AT 桥。建议在图表旁提供文本摘要。
