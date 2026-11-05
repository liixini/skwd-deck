use super::*;

fn write_pack(directory: &Path, body: &str) -> PathBuf {
    for file in [
        "runtime.so",
        "lineart.onnx",
        "depth.onnx",
        "subject.onnx",
        "character.onnx",
        "salient.onnx",
        "faces.onnx",
        "heads.onnx",
    ] {
        std::fs::write(directory.join(file), b"model").unwrap();
    }
    let path = directory.join("pack.json");
    std::fs::write(&path, body).unwrap();
    path
}

const PACK: &str = r#"{
    "version": 3,
    "runtime": "runtime.so",
    "device": "cpu",
    "lineart": "lineart.onnx",
    "depth": "depth.onnx",
    "subject": "subject.onnx",
    "character": "character.onnx",
    "salient": "salient.onnx",
    "faces": "faces.onnx",
    "heads": "heads.onnx",
    "subject_classes": [0, 15, 80],
    "subject_confidence": 0.5
}"#;

#[test]
fn resolves_relative_files() {
    let directory = tempfile::tempdir().unwrap();
    let pack = Pack::read(&write_pack(directory.path(), PACK)).unwrap();
    let root = directory.path().canonicalize().unwrap();
    assert_eq!(pack.lineart, root.join("lineart.onnx"));
    assert_eq!(pack.device, Device::Cpu);
    assert!(pack.subject_classes.contains(&80));
}

#[test]
fn depth_model_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let pack = Pack::read(&write_pack(directory.path(), PACK)).unwrap();
    assert_eq!(pack.depth_model, DepthModel::AnythingV2);
    assert_eq!(pack.depth_side, None);
}

#[test]
fn reads_depth_model() {
    let directory = tempfile::tempdir().unwrap();
    let body = PACK.replace(
        "\"depth\": \"depth.onnx\",",
        "\"depth\": \"depth.onnx\", \"depth_model\": \"depth-anything-3\", \"depth_side\": 1008,",
    );
    let pack = Pack::read(&write_pack(directory.path(), &body)).unwrap();
    assert_eq!(pack.depth_model, DepthModel::Anything3);
    assert_eq!(pack.depth_side.map(NonZeroU32::get), Some(1008));
}

#[test]
fn rejects_unknown_model() {
    let directory = tempfile::tempdir().unwrap();
    let body = PACK.replace(
        "\"depth\": \"depth.onnx\",",
        "\"depth\": \"depth.onnx\", \"depth_model\": \"depth-pro\",",
    );
    assert!(Pack::read(&write_pack(directory.path(), &body)).is_err());
}

#[test]
fn rejects_zero_side() {
    let directory = tempfile::tempdir().unwrap();
    let body =
        PACK.replace("\"depth\": \"depth.onnx\",", "\"depth\": \"depth.onnx\", \"depth_side\": 0,");
    assert!(Pack::read(&write_pack(directory.path(), &body)).is_err());
}

#[test]
fn rejects_missing_model() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_pack(directory.path(), PACK);
    std::fs::remove_file(directory.path().join("depth.onnx")).unwrap();
    assert!(Pack::read(&path).is_err());
}

#[test]
fn rejects_old_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_pack(directory.path(), &PACK.replace("\"version\": 3", "\"version\": 2"));
    assert!(Pack::read(&path).is_err());
}

#[test]
fn rejects_unknown_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path =
        write_pack(directory.path(), &PACK.replace("\"device\"", "\"python\": \"x\", \"device\""));
    assert!(Pack::read(&path).is_err());
}

#[test]
fn extras_default_empty() {
    let directory = tempfile::tempdir().unwrap();
    let pack = Pack::read(&write_pack(directory.path(), PACK)).unwrap();
    assert_eq!(pack.webgpu, None);
    assert!(pack.lineart_cpu_nodes.is_empty());
    assert_eq!(pack.files().count(), 8);
}

const WEBGPU: &str = r#""device": "webgpu", "webgpu": "webgpu.so",
    "lineart_cpu_nodes": ["/head/Conv", "/tail/Add"],"#;

#[test]
fn reads_device_extras() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("webgpu.so"), b"model").unwrap();
    let pack =
        Pack::read(&write_pack(directory.path(), &PACK.replace("\"device\": \"cpu\",", WEBGPU)))
            .unwrap();
    let root = directory.path().canonicalize().unwrap();
    assert_eq!(pack.device, Device::Webgpu);
    assert_eq!(pack.lineart_cpu_nodes, ["/head/Conv", "/tail/Add"]);
    let files: Vec<&Path> = pack.files().collect();
    assert_eq!(files.len(), 9);
    assert!(files.contains(&root.join("webgpu.so").as_path()));
}

#[test]
fn rejects_missing_extra() {
    let directory = tempfile::tempdir().unwrap();
    let body = PACK.replace("\"device\": \"cpu\",", WEBGPU);
    assert!(Pack::read(&write_pack(directory.path(), &body)).is_err());
}

#[test]
fn webgpu_needs_plugin() {
    let directory = tempfile::tempdir().unwrap();
    let body = PACK.replace("\"device\": \"cpu\"", "\"device\": \"webgpu\"");
    assert!(Pack::read(&write_pack(directory.path(), &body)).is_err());
    let auto = PACK.replace("\"device\": \"cpu\"", "\"device\": \"auto\"");
    assert_eq!(Pack::read(&write_pack(directory.path(), &auto)).unwrap().device, Device::Auto);
}
