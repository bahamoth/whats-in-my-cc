use assert_cmd::Command;

#[test]
fn help_shows_subcommands() {
    let assert = Command::cargo_bin("witmcc")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for sub in ["init-db", "ingest", "serve"] {
        assert!(
            out.contains(sub),
            "missing subcommand in help: {sub}\n{out}"
        );
    }
}
