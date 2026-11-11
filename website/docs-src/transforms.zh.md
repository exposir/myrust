`Transform` 有 translation、scale、rotation。`Context::transform` 对内部构造的控件应用作用域；display list 也支持 `TransformPush`/`TransformPop`。

```rust
use myrust_core::ui::{Point, Transform};

c.transform(
    Transform {
        translation: Point::new(12.0, 0.0),
        scale: Point::new(1.2, 1.2),
        rotation: 0.15,
    },
    |c| {
        container(c).size(80.0, 40.0).draw([ /* commands */ ]);
    },
);
```

transform 按节点各自原点应用，不是围绕所有 children 的整组变换。嵌套作用域近似组合 translation、scale、rotation。

## Skew and 3D

没有 skew、perspective、3D rotation 或任意 4x4 矩阵。`Transform` 只有上述三个字段；rasterizer 使用 2D affine。

没有 transform-origin 属性，原点是节点布局原点。非均匀 scale 叠加 rotation 可能产生公开 `Transform` 无法精确表达的几何。

## Coordinates, clips, and overlays

Transform 影响绘制几何，且对 `Context::transform` 作用域影响指针命中：指针逆变换到节点局部坐标。`ScissorPush` 矩形会被 rasterizer 变换；scissor 矩形不会为 children 创建通用坐标空间。

overlay 使用自己的顶层与锚点矩形。overlay 内容内的 transform 正常应用，但 overlay 定位本身不是公开 transform。

## Animating transforms

没有 transform transition 对象。用 `animate_with` 或 `TransitionState` progress 动画 translation/scale/rotation 标量，再每帧重建 `Transform`。改变 transform 不会自动请求帧。

命中区域使用 `Context::transform` 记录的变换；原始 `TransformPush` 命令不更新 hit-testing 元数据。

## Rendering

CPU presenter 对几何、文字与 SVG path 应用 translation/scale/rotation。支持矩形 scissor，不支持的命令会拒绝，而不是模拟 GPU 效果。

可选 wgpu backend 通过同一薄 renderer 接口消费受支持几何；没有公开的 transform 专用 GPU feature 或 shader。CPU presenter 仍不支持 image。
