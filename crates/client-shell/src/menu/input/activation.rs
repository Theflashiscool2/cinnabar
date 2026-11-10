//! Focused menu activation and the text fields that commit on Enter.
use super::{MenuRuntime, UiPresentationRuntime};
use launcher::menu::{MenuAction, MenuField};

/// Activates the focused control with its normal press sound.
pub(super) fn select_focused(menu: &mut MenuRuntime, presentation: &UiPresentationRuntime) {
    if let Some(action) = menu.focused_action() {
        presentation.play_menu_press_sound(action);
    }
    menu.activate_focused();
}

/// Commits a field with a dedicated action, otherwise activates menu focus.
pub(super) fn enter(menu: &mut MenuRuntime, presentation: &UiPresentationRuntime) {
    match menu.field {
        Some(MenuField::SkinName) => menu.activate(MenuAction::DressingRoom(
            launcher::dressing_room::Action::SaveRename,
        )),
        Some(MenuField::JavaClientId) => menu.activate(MenuAction::JavaAccount(
            launcher::java_accounts::Action::SaveSetup,
        )),
        _ => select_focused(menu, presentation),
    }
}
