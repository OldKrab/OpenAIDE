use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

/// Installs the native menu. Every item id is a `desktop-command` the shared
/// Frontend handles, so the WebView shortcuts on other platforms stay the same set.
pub(crate) fn install(app: &mut tauri::App) -> tauri::Result<()> {
    let new_task = MenuItemBuilder::with_id("new-task", "New Task")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let open_project = MenuItemBuilder::with_id("open-project", "Add Project Folder…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let switch_computer = MenuItemBuilder::with_id("switch-computer", "Switch Computer…")
        .accelerator("CmdOrCtrl+Shift+S")
        .build(app)?;
    let check_for_updates =
        MenuItemBuilder::with_id("check-for-updates", "Check for Updates…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit OpenAIDE")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    let application = SubmenuBuilder::new(app, "OpenAIDE")
        .items(&[
            &settings,
            &check_for_updates,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ])
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .items(&[&new_task, &open_project, &PredefinedMenuItem::separator(app)?, &switch_computer])
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .items(&[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ])
        .build()?;
    app.set_menu(
        MenuBuilder::new(app)
            .items(&[&application, &file, &edit])
            .build()?,
    )?;
    Ok(())
}
