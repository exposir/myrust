`Transform` has translation, scale and rotation. `Context::transform` applies a scope to the widgets constructed inside; display lists also support `TransformPush`/`TransformPop`.

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

The transform is applied per constructed node at its own origin, not as one group around all children. Nested scopes compose translation, scale and rotation approximately.

## Skew and 3D

No skew, perspective, 3D rotation or arbitrary 4x4 matrix exists. `Transform` has only the three fields above; the rasterizer uses a 2D affine matrix.

There is no transform-origin property, so the origin is the node's layout origin. Non-uniform scale combined with rotation can produce geometry the public `Transform` cannot express exactly.

## Coordinates, clips, and overlays

Transforms affect painted geometry and (for `Context::transform` scopes) pointer hit-testing: the pointer is inverse-transformed into node-local coordinates. `ScissorPush` rectangles are transformed by the rasterizer; scissor rectangles do not create a general coordinate space for children.

Overlays use their own top layer and anchor rectangles. A transform inside overlay content applies normally, but the overlay positioning itself is not expressed as a public transform.

## Animating transforms

There is no transform transition object. Animate translation/scale/rotation scalars with `animate_with` or `TransitionState` progress, then rebuild `Transform` each frame. Changing a transform does not automatically request frames.

The hit region uses the transform recorded for `Context::transform`; raw `TransformPush` commands do not update hit-testing metadata.

## Rendering

The CPU presenter applies translation/scale/rotation to geometry, text and SVG paths. It supports rectangular scissors and rejects unsupported commands rather than emulating GPU effects.

The optional wgpu backend consumes the same thin renderer interface for supported geometry; no transform-specific GPU feature or shader is public. Images remain unsupported by the CPU presenter.
