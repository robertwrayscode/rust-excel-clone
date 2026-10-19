//! System clipboard access for menu/ribbon commands (keyboard shortcuts use
//! egui's copy/cut/paste events).

use std::cell::RefCell;

thread_local! {
    static CLIP: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut arboard::Clipboard) -> Option<R>) -> Option<R> {
    CLIP.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = arboard::Clipboard::new().ok();
        }
        c.as_mut().and_then(f)
    })
}

pub fn get_text() -> Option<String> {
    with(|c| c.get_text().ok()).map(|t| t.replace("\r\n", "\n"))
}

pub fn set_text(text: &str) {
    with(|c| c.set_text(text.to_string()).ok());
}
