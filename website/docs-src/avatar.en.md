`avatar(c, "id", label, initials, size, shape)` draws an initials avatar. `avatar_image(c, "id", label, data, size, shape)` draws caller-decoded RGBA pixels, optionally circle-masked.

```rust
avatar(c, "user", "Ada Lovelace", "AL", 40.0, AvatarShape::Circle);
```

## Accessibility

Both variants emit `Role::Label`: label = the accessible name (`label` argument), value `"image"` when pixels are present, otherwise the visible initials. There is no Avatar role, online/status indicator, image alt text beyond `label`, or AT bridge.

The avatar is not focusable and has no fallback/error node. Missing image data simply falls back to initials or a placeholder.
