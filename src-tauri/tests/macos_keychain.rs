#![cfg(target_os = "macos")]

#[test]
fn native_keychain_persists_across_entry_instances() {
    let user = format!("bundle-check-{}", uuid::Uuid::new_v4());
    let entry = keyring::Entry::new("BilibiliVideoDownloader-CI", &user).unwrap();
    entry.set_password("test-value-not-a-real-cookie").unwrap();
    let second = keyring::Entry::new("BilibiliVideoDownloader-CI", &user).unwrap();
    let result = second.get_password();
    entry.delete_credential().unwrap();
    assert_eq!(result.unwrap(), "test-value-not-a-real-cookie");
    assert!(matches!(
        second.get_password(),
        Err(keyring::Error::NoEntry)
    ));
}
