use assert_cmd::Command;
use tempfile::tempdir;

const SIMPLE_SVG: &str = r#"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
  <path d="M1 1 H9 V9 H1 Z" fill="black"/>
</svg>
"#;

#[test]
fn cli_writes_png_and_metadata() {
    let temp = tempdir().unwrap();
    let svg_path = temp.path().join("icon.svg");
    let png_path = temp.path().join("icon.msdf.png");
    let json_path = temp.path().join("icon.msdf.json");
    std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(&svg_path)
        .arg("--size")
        .arg("16")
        .arg("--output")
        .arg(&png_path)
        .arg("--metadata")
        .arg(&json_path)
        .assert()
        .success();

    assert!(png_path.exists());
    assert!(json_path.exists());

    let metadata = std::fs::read_to_string(json_path).unwrap();
    assert!(metadata.contains("\"format\": \"msdf-rgb8\""));
    assert!(metadata.contains("\"width\": 16"));
}

#[test]
fn cli_rejects_bad_size() {
    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg("missing.svg")
        .arg("--size")
        .arg("0")
        .arg("--output")
        .arg("out.png")
        .assert()
        .failure();
}
