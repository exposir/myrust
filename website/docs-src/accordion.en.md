`accordion(c, "id", &mut state, sections)` draws stacked expandable sections. `AccordionState` stores open ids and `AccordionMode` (Single or Multiple); `AccordionSection` content is a text string.

```rust
use myrust_core::ui::{accordion, AccordionMode, AccordionSection, AccordionState};

let sections = [AccordionSection::new("one", "One", "First body")];
accordion(c, "acc", &mut app.accordion, &sections);
```

## Without a look

There is no custom section content, icon slot or low-level accordion builder. Section bodies are text only; put arbitrary widgets outside the accordion.

No height animation, disabled section, lazy content or nested accordion coordination exists.

## Accessibility

Each header emits `Role::Disclosure`, label = section title, value `"expanded"`/`"collapsed"`, focused boolean. The root emits `Role::Group` with the id.

There is no Accordion role, section position/level, controls relationship or AT bridge.
