//! Control-id state migration helpers.

use std::collections::HashMap;

/// Moves a map entry from `old` to `new`.
///
/// Returns `false` when the ids are equal, the old key is missing, or the new
/// key already exists. In all failure cases the map is left unchanged.
pub fn migrate_value<T>(map: &mut HashMap<String, T>, old: &str, new: &str) -> bool {
    if old == new || !map.contains_key(old) || map.contains_key(new) {
        return false;
    }
    if let Some(value) = map.remove(old) {
        map.insert(new.to_string(), value);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::event::{Key, UiEvent};
    use crate::layout::Size;

    #[test]
    fn migrate_value_moves_only_when_the_target_is_free() {
        let mut map = HashMap::new();
        map.insert("old".to_string(), "value".to_string());
        assert!(migrate_value(&mut map, "old", "new"));
        assert!(!map.contains_key("old"));
        assert_eq!(map.get("new").map(String::as_str), Some("value"));
        assert!(!migrate_value(&mut map, "missing", "next"));
        map.insert("target".to_string(), "existing".to_string());
        assert!(!migrate_value(&mut map, "new", "target"));
        assert_eq!(map.get("new").map(String::as_str), Some("value"));
        assert_eq!(map.get("target").map(String::as_str), Some("existing"));
    }

    #[test]
    fn context_migrates_text_cursor_and_focus_to_the_new_id() {
        let size = Size::new(240.0, 100.0);
        let mut context = Context::new(size);
        let mut value = String::new();
        context.begin_frame(size);
        context.request_focus("old");
        context.set_input(UiEvent::TextInput {
            text: "abc".to_string(),
        });
        context.text_field("old", &mut value);
        context.produce().unwrap();
        context.end_frame();

        context.begin_frame(size);
        assert!(context.migrate_state("old", "new"));
        context.set_input(UiEvent::Key {
            key: Key::Backspace,
        });
        let state = context.text_field("new", &mut value);
        assert_eq!(value, "ab");
        assert!(state.focused);
        assert!(!context.migrate_state("old", "new"));
    }
}
