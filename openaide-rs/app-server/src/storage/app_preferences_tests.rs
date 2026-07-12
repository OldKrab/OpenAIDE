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
}

#[test]
fn app_preferences_update_persists_composer_shortcut() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();

    let updated = store
        .update_app_preferences(AppPreferencesPatch {
            composer_submit_shortcut: ComposerSubmitShortcut::ModEnter,
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
