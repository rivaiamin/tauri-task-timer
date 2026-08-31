// ponytail: Linux clipboard is owned by the setter until Clipboard is dropped.

pub fn set_text(
    handle: &mut Option<arboard::Clipboard>,
    text: String,
) -> Result<(), arboard::Error> {
    if handle.is_none() {
        *handle = Some(arboard::Clipboard::new()?);
    }
    handle.as_mut().unwrap().set_text(text)
}
