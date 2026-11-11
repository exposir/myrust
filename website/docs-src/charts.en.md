`bar_chart(c, "id", values, height, min, max)` and `line_chart(...)` draw a fixed-height chart from `&[f32]`. `ChartResponse` reports the plot rect, effective min/max, point count and kind.

```rust
let values = [0.2, 0.8, 0.4, 0.9];
bar_chart(c, "bars", &values, 120.0, Some(0.0), Some(1.0));
```

## Scales

Min/max can be supplied; otherwise the finite data range is used. Non-finite points are filtered. The scale is linear; there is no log/time/band scale or tick generation.

### Wide ranges and signed data

Negative/positive values work within the linear min/max; there is no zero-baseline policy, log scale, nice ticks or automatic padding beyond the data extremes.

## The plot and its axes

`ChartResponse::plot` is the chart-local plot rectangle. No axes, ticks, labels, grid lines or legends are drawn; the caller can add text/line commands from the response.

## Marks

Bar charts draw one rectangle per point; line charts draw connected segments. There are no point markers, hover targets, tooltips or per-mark colors.

## Lines and areas

Only a polyline is drawn; no area fill, curve interpolation or gap policy. Missing/non-finite points are dropped rather than creating gaps.

### Live data

The caller rebuilds the values each frame. There is no streaming buffer, animation between datasets or auto-scroll.

## Stacks

No stacked/grouped bars or multi-series support exists. Draw multiple charts or custom rectangles.

## Pies and donuts

No pie/donut/radial chart exists. Custom `DrawCommand::SvgPath` or circles can approximate one.

## Selection

No hover/click/brush selection, cursor readout or selected index is returned. Add pointer regions/overlays externally.

## Accessibility

The chart root emits `Role::Group` with the chart id. There are no per-mark, axis or data-summary AccessNodes, no chart role and no AT bridge. Provide a text summary beside the chart for nonvisual users.
