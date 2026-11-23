#[cfg(unix)]
#[test]
fn plugin_commands_list_and_check() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let data = tempfile::tempdir().unwrap();
    let plugin = data.path().join("plugins/demo");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(
        plugin.join("plugin.sh"),
        "#!/bin/sh\nIFS= read -r request\nprintf '%s\\n' '{\"id\":\"demo\",\"ummp_api\":1}'\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(plugin.join("plugin.sh"))
        .unwrap()
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(plugin.join("plugin.sh"), permissions).unwrap();
    std::fs::write(
        plugin.join("ummp-plugin.toml"),
        "name='demo'\nversion='1.0.0'\nkind='adapter'\ncommand=['plugin.sh']\nummp_api=1\n",
    )
    .unwrap();

    let list = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "plugins", "list"])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(value["result"]["plugins"][0]["name"], "demo");

    let check = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "plugins", "check", plugin.to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(value["result"]["valid"], true);
}
