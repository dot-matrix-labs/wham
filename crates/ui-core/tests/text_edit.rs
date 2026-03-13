use ui_core::text::TextBuffer;

#[test]
fn undo_redo_text() {
    let mut buf = TextBuffer::new("hi");
    buf.insert_text("!");
    assert_eq!(buf.text(), "hi!");
    assert!(buf.undo());
    assert_eq!(buf.text(), "hi");
    assert!(buf.redo());
    assert_eq!(buf.text(), "hi!");
}

