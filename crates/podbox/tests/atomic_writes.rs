mod sandbox;

use sandbox::Sandbox;

/// State files must be swapped into place atomically: after
/// `snapshot create` the metadata is complete and no temp files remain.
/// (A direct truncating write could leave a torn file behind on crash.)
#[test]
fn snapshot_metadata_is_written_completely() {
    let sb = Sandbox::new("atomic");
    sb.write_config("mybox", "mybox");

    let out = sb.run(&["snapshot", "create", "mybox", "--tag", "t1"]);
    assert!(
        out.status.success(),
        "snapshot create failed: {}",
        sandbox::text(&out)
    );

    let dir = sb.root.join("config/podbox/snapshots/mybox");
    let entries: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["t1.toml".to_string()]);
    let meta = std::fs::read_to_string(dir.join("t1.toml")).unwrap();
    assert!(meta.contains("tag = \"t1\""), "metadata torn: {meta:?}");
    assert!(meta.contains("image = \""), "metadata torn: {meta:?}");
}
