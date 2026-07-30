use super::*;

#[test]
fn missing_preferences_return_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();

    assert_eq!(
        store
            .read_app_preferences()
            .unwrap()
            .composer_submit_shortcut,
        ComposerSubmitShortcut::Enter
    );
    assert_eq!(
        store.read_app_preferences().unwrap().theme,
        AppTheme::System
    );
}

#[test]
fn legacy_preferences_without_theme_follow_the_operating_system() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    std::fs::write(
        store.settings_dir().join("app_preferences.json"),
        r#"{"composerSubmitShortcut":"modEnter"}"#,
    )
    .unwrap();

    let preferences = store.read_app_preferences().unwrap();

    assert_eq!(
        preferences.composer_submit_shortcut,
        ComposerSubmitShortcut::ModEnter
    );
    assert_eq!(preferences.theme, AppTheme::System);
}

#[test]
fn app_preferences_update_persists_composer_shortcut() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();

    let updated = store
        .update_app_preferences(AppPreferencesPatch {
            composer_submit_shortcut: Some(ComposerSubmitShortcut::ModEnter),
            theme: None,
        })
        .unwrap();

    assert_eq!(
        updated.composer_submit_shortcut,
        ComposerSubmitShortcut::ModEnter
    );
    assert_eq!(
        store
            .read_app_preferences()
            .unwrap()
            .composer_submit_shortcut,
        ComposerSubmitShortcut::ModEnter
    );
}

#[test]
fn app_preferences_update_persists_theme_without_replacing_other_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    store
        .update_app_preferences(AppPreferencesPatch {
            composer_submit_shortcut: Some(ComposerSubmitShortcut::ModEnter),
            theme: None,
        })
        .unwrap();

    let updated = store
        .update_app_preferences(AppPreferencesPatch {
            composer_submit_shortcut: None,
            theme: Some(AppTheme::Dark),
        })
        .unwrap();

    assert_eq!(updated.theme, AppTheme::Dark);
    assert_eq!(
        updated.composer_submit_shortcut,
        ComposerSubmitShortcut::ModEnter
    );
    assert_eq!(store.read_app_preferences().unwrap(), updated);
}
