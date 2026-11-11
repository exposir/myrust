`toasts(c, "id", &mut queue)` paints the caller-owned `ToastQueue` at the top-right. `Toast::new(message)` and `.action(label)` build entries; `push` takes `&Context` to start the clock.

```rust
let mut toast = Toast::new("Saved");
queue.push(c, toast);
let response = toasts(c, "toasts", &mut queue);
```

## Adding and closing toasts

`ToastQueue::push`/`push_message` append; `toasts` expires by `duration` and reports `ToastEvent { index, kind, action }` for timeout/close/action. The close `x` and optional action are Buttons.

There is no stacking limit, deduplication, queue reorder or manual remove by id beyond the returned events.

## In the background

Toasts expire only when `toasts` is called with a frame; there is no background timer. If the view stops rendering, queued toasts persist. Time uses `Context::frame_time`.

## Without a look

There is no low-level toast builder, custom placement, icon or rich body. `toasts` owns the rounded panel and top-right stacking; arbitrary content requires a custom overlay (which would not participate in queue expiry).

## Accessibility

The toast panel emits `Role::Group`, label `"toast"`, value = message. The message is a `Role::Label`; action/close are `Role::Button` with their labels.

There is no LiveRegion/Alert role, announcement event, timeout metadata or AT bridge. Screen readers are not notified.
