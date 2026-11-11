路由器是调用方持有的页面 id 历史加一组页面闭包。它不是 URL 路由器，也不创建窗口；只在当前视图内选择一个闭包构建。

```rust
use myrust_core::ui::{back_forward_buttons, router, text, Context, Page, RouterState};

fn home_page(c: &mut Context) { text(c, "Home"); }
fn about_page(c: &mut Context) { text(c, "About"); }

let pages = [
    Page::new("home", &home_page),
    Page::new("about", &about_page),
];
let buttons = back_forward_buttons(c, "nav-buttons", &mut app.router);
router(c, "pages", &mut app.router, &pages);
```

## Going to pages

`RouterState::push(page)` 截断 forward 历史并追加页面；`RouterState::current()` 返回活动 id。`router` 查找匹配 `Page` 并用同一 `Context` 调用其 `render` 闭包，否则绘制 “Page not found”。

没有 URL 解析、路由参数、query string 或外部导航。push/pop 完全由应用驱动；页面可在自己的控件里 push 另一页。

## Back and forward

`RouterState::back()` / `forward()` 在可能时移动索引；`can_back` / `can_forward` 报告可用性。`back_forward_buttons` 渲染两个 Button 节点，label 为 `back`/`forward`，value 为 `enabled`/`disabled`。

没有浏览器式 history API，除 `history()` 外没有可编辑页面 id 栈，也没有浏览器后退手势。禁用的历史按钮以 muted 绘制，但仍发出 `disabled` value 的 AccessNode；它不可键盘交互。

## Layouts

路由器没有 layout 对象或页面过渡布局。每个页面闭包构建自己需要的容器；router 根是 `Group`，label 为当前 page id。

切换页面可能在下一帧完全改变布局。没有保留页面树、滚动恢复或路由级动画曲线。需要显式矩形动画时调用方使用 `transition`。

## Pages keep their state

`RouterState` 保存 id，不保存页面状态。跨页面共享状态在调用方存活；逐页状态需由应用按 page id 键控。离开页面时不会自动保存。

没有 page cache、除选中闭包外的惰性页面构建或逐路由焦点恢复。`migrate_state` 可按 id 移动瞬态控件状态，但页面数据迁移由调用方负责。

## The focus and screen readers

router 发出一个 `Role::Group` 节点，label 为当前 page id；页面内容发出自己的节点。back/forward 是 `Role::Button`，label 为 `back`/`forward`，value 为 `enabled`/`disabled`。

router 不在导航时移动焦点、宣告新页面或暴露 landmark/live region。未连接平台 screen reader；这些只是元数据节点。

## Transitions

`RouterState` 自身没有过渡状态。`transition(c, &mut state, target)` 暴露 `TransitionFrame { rect, progress }` 供调用方动画布局矩形；使用帧时钟并遵守 reduce-motion。

页面交叉淡入、进入/退出动画、shared-element 过渡与历史方向过渡未实现。调用方需自行构建两种状态并显式动画。
