//! Notification settings scenario tests.
//!
//! Models an app settings screen with icon-decorated checkboxes for
//! notification channels and a radio group for theme preference.
//! Exercises icon rendering, checkbox interactivity, and radio group layout.

use ui_core::{
    batch::Material,
    icon::IconPack,
    theme::Theme,
    types::Vec2,
    ui::{Ui, WidgetKind},
    input::{InputEvent, Modifiers, PointerButton, PointerEvent},
};
use wham_test::Size;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Inline icon pack: bell, email, device, and palette icons on a 96×24 atlas.
fn notification_icon_pack() -> IconPack {
    let manifest = r#"{
        "name": "notifications",
        "texture_size": [96, 24],
        "icons": [
            { "name": "bell",    "x":  0, "y": 0, "w": 24, "h": 24 },
            { "name": "email",   "x": 24, "y": 0, "w": 24, "h": 24 },
            { "name": "device",  "x": 48, "y": 0, "w": 24, "h": 24 },
            { "name": "palette", "x": 72, "y": 0, "w": 24, "h": 24 }
        ]
    }"#;
    IconPack::from_manifest(manifest).expect("icon manifest must parse")
}

/// Run a single frame; returns the Ui (with batch accessible) after end_frame.
fn run_frame_capturing_ui(
    size: Size,
    build: impl FnOnce(&mut Ui),
) -> Ui {
    let w = size.width as f32;
    let h = size.height as f32;
    let mut ui = Ui::new(w, h, Theme::default_light());
    ui.begin_frame(vec![], w, h, 1.0, 0.0);
    build(&mut ui);
    ui.end_frame();
    ui
}

/// Run two frames: first to get layout, second with a click event.
/// Returns widgets from the second frame.
#[allow(dead_code)]
fn run_with_click(
    size: Size,
    click_pos: Vec2,
    mut build: impl FnMut(&mut Ui),
) -> Vec<ui_core::ui::WidgetInfo> {
    let w = size.width as f32;
    let h = size.height as f32;
    let mut ui = Ui::new(w, h, Theme::default_light());

    // Frame 1: no events — discover layout.
    ui.begin_frame(vec![], w, h, 1.0, 0.0);
    build(&mut ui);
    ui.end_frame();

    // Frame 2: click.
    let click_ev = PointerEvent {
        pos: click_pos,
        button: Some(PointerButton::Left),
        modifiers: Modifiers::default(),
    };
    let events = vec![
        InputEvent::PointerDown(click_ev),
        InputEvent::PointerUp(click_ev),
    ];
    ui.begin_frame(events, w, h, 1.0, 16.0);
    build(&mut ui);
    ui.end_frame();

    ui.widgets().to_vec()
}

// ---------------------------------------------------------------------------
// Notification settings view
// ---------------------------------------------------------------------------

fn notification_settings_view(
    ui: &mut Ui,
    email_on: &mut bool,
    push_on: &mut bool,
    sms_on: &mut bool,
    theme_idx: &mut usize,
) {
    ui.label("Notifications");
    ui.icon("bell", 20.0);
    ui.checkbox("Email notifications", email_on);
    ui.icon("email", 20.0);
    ui.checkbox("Push notifications", push_on);
    ui.icon("device", 20.0);
    ui.checkbox("SMS alerts", sms_on);

    ui.label("Appearance");
    ui.icon("palette", 20.0);
    let theme_options = vec![
        "System default".to_string(),
        "Light".to_string(),
        "Dark".to_string(),
    ];
    ui.radio_group("Theme", &theme_options, theme_idx);

    ui.button("Save changes");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn notification_icon_pack_parses_correctly() {
    let pack = notification_icon_pack();
    assert_eq!(pack.len(), 4, "icon pack should have 4 icons");
    assert!(pack.get("bell").is_some(), "bell icon must be present");
    assert!(pack.get("email").is_some(), "email icon must be present");
    assert!(pack.get("device").is_some(), "device icon must be present");
    assert!(pack.get("palette").is_some(), "palette icon must be present");
}

#[test]
fn notification_icons_emit_icon_atlas_draw_commands() {
    let size = Size { width: 480, height: 800 };
    let (mut email_on, mut push_on, mut sms_on) = (true, false, false);
    let mut theme_idx = 0usize;

    let ui = run_frame_capturing_ui(size, |ui| {
        ui.set_icon_pack(notification_icon_pack());
        notification_settings_view(ui, &mut email_on, &mut push_on, &mut sms_on, &mut theme_idx);
    });

    let icon_atlas_commands: Vec<_> = ui
        .batch()
        .commands
        .iter()
        .filter(|cmd| cmd.material == Material::IconAtlas)
        .collect();

    assert!(
        !icon_atlas_commands.is_empty(),
        "at least one IconAtlas draw command must be emitted when icons are rendered"
    );
}

#[test]
fn notification_email_checkbox_toggles_on_click() {
    let size = Size { width: 480, height: 800 };

    // First we need the checkbox rect. Run one frame to discover it.
    let w = size.width as f32;
    let h = size.height as f32;
    let (mut email_on, mut push_on, mut sms_on) = (false, false, false);
    let mut theme_idx = 0usize;
    let mut ui = Ui::new(w, h, Theme::default_light());

    ui.begin_frame(vec![], w, h, 1.0, 0.0);
    ui.set_icon_pack(notification_icon_pack());
    notification_settings_view(&mut ui, &mut email_on, &mut push_on, &mut sms_on, &mut theme_idx);
    ui.end_frame();

    let checkbox_rect = ui.widgets()
        .iter()
        .find(|w| w.label == "Email notifications")
        .expect("Email notifications checkbox not found")
        .rect;

    // Click the checkbox.
    let click_ev = PointerEvent {
        pos: checkbox_rect.center(),
        button: Some(PointerButton::Left),
        modifiers: Modifiers::default(),
    };
    let events = vec![
        InputEvent::PointerDown(click_ev),
        InputEvent::PointerUp(click_ev),
    ];
    ui.begin_frame(events, w, h, 1.0, 16.0);
    ui.set_icon_pack(notification_icon_pack());
    notification_settings_view(&mut ui, &mut email_on, &mut push_on, &mut sms_on, &mut theme_idx);
    ui.end_frame();

    assert!(email_on, "clicking the checkbox must toggle it on");
}

#[test]
fn notification_radio_group_renders_all_theme_options() {
    let size = Size { width: 480, height: 800 };
    let (mut email_on, mut push_on, mut sms_on) = (false, false, false);
    let mut theme_idx = 0usize;

    let ui = run_frame_capturing_ui(size, |ui| {
        ui.set_icon_pack(notification_icon_pack());
        notification_settings_view(ui, &mut email_on, &mut push_on, &mut sms_on, &mut theme_idx);
    });

    let radio_widgets: Vec<_> = ui.widgets()
        .iter()
        .filter(|w| w.kind == WidgetKind::Radio)
        .collect();

    assert_eq!(radio_widgets.len(), 3, "three theme radio options: System default, Light, Dark");
    assert!(
        radio_widgets.iter().any(|w| w.label == "System default"),
        "System default option must be present"
    );
    assert!(
        radio_widgets.iter().any(|w| w.label == "Light"),
        "Light option must be present"
    );
    assert!(
        radio_widgets.iter().any(|w| w.label == "Dark"),
        "Dark option must be present"
    );
}

#[test]
fn notification_save_button_emits_primary_colored_quad() {
    let size = Size { width: 480, height: 800 };
    let (mut email_on, mut push_on, mut sms_on) = (false, false, false);
    let mut theme_idx = 0usize;

    let ui = run_frame_capturing_ui(size, |ui| {
        ui.set_icon_pack(notification_icon_pack());
        notification_settings_view(ui, &mut email_on, &mut push_on, &mut sms_on, &mut theme_idx);
    });

    // The "Save changes" button should emit a quad with the theme's primary
    // color (light theme: rgba(0.2, 0.45, 0.9, 1.0) — blue >> red).
    // We verify that at least one solid-colored vertex has b > r.
    let has_primary_quad = ui.batch().vertices.iter().any(|v| v.color.b > v.color.r + 0.2);

    assert!(
        has_primary_quad,
        "at least one vertex must use the primary blue color (button or focus ring)"
    );
}

#[test]
fn notification_settings_screenshot() {
    use std::cell::RefCell;
    use wham_test::{save_screenshot, Size};

    let email_on = RefCell::new(true);
    let push_on  = RefCell::new(false);
    let sms_on   = RefCell::new(false);
    let theme    = RefCell::new(0usize);

    save_screenshot("notification_settings", Size { width: 480, height: 720 }, |ui| {
        ui.set_icon_pack(notification_icon_pack());
        notification_settings_view(
            ui,
            &mut email_on.borrow_mut(),
            &mut push_on.borrow_mut(),
            &mut sms_on.borrow_mut(),
            &mut theme.borrow_mut(),
        );
    });
}
