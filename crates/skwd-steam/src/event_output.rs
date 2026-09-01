use std::io::Write;

pub fn emit(id: &str, status: &str, progress: f64, message: &str) {
    emit_with_folder(id, status, progress, message, "");
}

pub fn emit_with_folder(id: &str, status: &str, progress: f64, message: &str, folder: &str) {
    let line = serde_json::json!({
        "id": id,
        "status": status,
        "progress": progress,
        "message": message,
        "folder": folder,
    });
    println!("{line}");
    let _ = std::io::stdout().flush();
}
