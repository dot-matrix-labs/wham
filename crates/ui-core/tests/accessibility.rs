use ui_core::theme::Theme;
use ui_core::ui::Ui;

#[test]
fn accessibility_tree_contains_widgets() {
    let mut ui = Ui::new(800.0, 600.0, Theme::default_light());
    ui.begin_frame(Vec::new(), 800.0, 600.0, 1.0, 0.0);
    ui.label("Title");
    let mut text = ui_core::text::TextBuffer::new("hello");
    ui.text_input("Name", &mut text, "name");
    let tree = ui.end_frame();
    let nodes = tree.flatten();
    assert!(nodes.len() >= 3);
}

