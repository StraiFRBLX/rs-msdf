use assert_cmd::Command;
use base64::Engine;
use serde_json::Value;
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
fn cli_writes_self_contained_json_export() {
    let temp = tempdir().unwrap();
    let svg_path = temp.path().join("icon.svg");
    let json_path = temp.path().join("icon.msdf.json");
    std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(&svg_path)
        .arg("--size")
        .arg("16")
        .arg("--output")
        .arg(&json_path)
        .assert()
        .success();

    assert!(json_path.exists());

    let export: Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(export["kind"], "rs-msdf");
    assert_eq!(export["version"], 2);
    assert_eq!(export["format"], "msdf-rgb8");
    assert_eq!(export["encoding"], "base64");
    assert_eq!(export["channels"], "rgb");
    assert_eq!(export["bytes_per_pixel"], 3);
    assert_eq!(export["width"], 16);
    assert_eq!(export["height"], 16);

    let data_len = export["data_len"].as_u64().unwrap() as usize;
    let uncompressed_data_len = export["uncompressed_data_len"].as_u64().unwrap() as usize;
    assert_eq!(uncompressed_data_len, 16 * 16 * 3);
    assert_eq!(data_len, uncompressed_data_len);

    let data = export["data"].as_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .unwrap();
    assert_eq!(decoded.len(), data_len);
}

#[test]
fn cli_writes_compressed_json_export_when_requested() {
    let temp = tempdir().unwrap();
    let svg_path = temp.path().join("icon.svg");
    let json_path = temp.path().join("icon.msdf.json");
    std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(&svg_path)
        .arg("--size")
        .arg("16")
        .arg("--compress")
        .arg("--output")
        .arg(&json_path)
        .assert()
        .success();

    let export: Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(export["encoding"], "base64+zstd+png-filter");
    let data_len = export["data_len"].as_u64().unwrap() as usize;
    let uncompressed_data_len = export["uncompressed_data_len"].as_u64().unwrap() as usize;
    assert_eq!(uncompressed_data_len, 16 * 16 * 3);

    let data = export["data"].as_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .unwrap();
    assert_eq!(decoded.len(), data_len);
    let decompressed = oxiarc_zstd::decode_all(&decoded).unwrap();
    assert_eq!(decompressed.len(), 16 * (16 * 3 + 1));
    assert!(decompressed.len() > uncompressed_data_len);
}

#[test]
fn cli_accepts_short_aliases() {
    let temp = tempdir().unwrap();
    let svg_path = temp.path().join("icon.svg");
    let json_path = temp.path().join("icon.mtsdf.json");
    std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(&svg_path)
        .arg("-s")
        .arg("16")
        .arg("-m")
        .arg("mtsdf")
        .arg("-r")
        .arg("5")
        .arg("-c")
        .arg("-l")
        .arg("7")
        .arg("-o")
        .arg(&json_path)
        .assert()
        .success();

    let export: Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(export["format"], "mtsdf-rgba8");
    assert_eq!(export["range_px"], 5.0);
    assert_eq!(export["encoding"], "base64+zstd+png-filter");
}

#[test]
fn cli_writes_mtsdf_json_export() {
    let temp = tempdir().unwrap();
    let svg_path = temp.path().join("icon.svg");
    let json_path = temp.path().join("icon.mtsdf.json");
    std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(&svg_path)
        .arg("--size")
        .arg("16")
        .arg("--mode")
        .arg("mtsdf")
        .arg("--output")
        .arg(&json_path)
        .assert()
        .success();

    let export: Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(export["format"], "mtsdf-rgba8");
    assert_eq!(export["channels"], "rgba");
    assert_eq!(export["bytes_per_pixel"], 4);
    assert_eq!(export["encoding"], "base64");

    let uncompressed_data_len = export["uncompressed_data_len"].as_u64().unwrap() as usize;
    assert_eq!(uncompressed_data_len, 16 * 16 * 4);
}

#[test]
fn cli_expands_glob_inputs_into_out_dir() {
    let temp = tempdir().unwrap();
    let input_dir = temp.path().join("icons");
    let output_dir = temp.path().join("out");
    std::fs::create_dir(&input_dir).unwrap();
    std::fs::write(input_dir.join("a.svg"), SIMPLE_SVG).unwrap();
    std::fs::write(input_dir.join("b.svg"), SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(glob_pattern(&input_dir))
        .arg("-s")
        .arg("16")
        .arg("-d")
        .arg(&output_dir)
        .arg("-f")
        .arg("json")
        .assert()
        .success();

    assert!(output_dir.join("a.msdf.json").exists());
    assert!(output_dir.join("b.msdf.json").exists());
}

#[test]
fn cli_rejects_glob_with_output_file() {
    let temp = tempdir().unwrap();
    let input_dir = temp.path().join("icons");
    let output_dir = temp.path().join("out");
    std::fs::create_dir(&input_dir).unwrap();
    std::fs::write(input_dir.join("a.svg"), SIMPLE_SVG).unwrap();
    std::fs::write(input_dir.join("b.svg"), SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(glob_pattern(&input_dir))
        .arg("-s")
        .arg("16")
        .arg("-d")
        .arg(&output_dir)
        .arg("-f")
        .arg("json")
        .arg("-o")
        .arg(temp.path().join("one.json"))
        .assert()
        .failure();
}

#[test]
fn cli_rejects_glob_with_metadata_file() {
    let temp = tempdir().unwrap();
    let input_dir = temp.path().join("icons");
    let output_dir = temp.path().join("out");
    std::fs::create_dir(&input_dir).unwrap();
    std::fs::write(input_dir.join("a.svg"), SIMPLE_SVG).unwrap();
    std::fs::write(input_dir.join("b.svg"), SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(glob_pattern(&input_dir))
        .arg("-s")
        .arg("16")
        .arg("-d")
        .arg(&output_dir)
        .arg("-f")
        .arg("png")
        .arg("-M")
        .arg(temp.path().join("meta.json"))
        .assert()
        .failure();
}

#[test]
fn cli_rejects_unknown_output_extension() {
    let temp = tempdir().unwrap();
    let svg_path = temp.path().join("icon.svg");
    let output_path = temp.path().join("icon.msdf.txt");
    std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

    Command::cargo_bin("rs-msdf")
        .unwrap()
        .arg(&svg_path)
        .arg("--size")
        .arg("16")
        .arg("--output")
        .arg(&output_path)
        .assert()
        .failure();
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

fn glob_pattern(dir: &std::path::Path) -> String {
    format!(
        "{}/{}",
        dir.display().to_string().replace('\\', "/"),
        "*.svg"
    )
}
