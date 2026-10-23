//! Встраивает иконку в Windows-программу.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../../tools/windows/yatype.rc");
    println!("cargo:rerun-if-changed=../../tools/windows/yatype.ico");
    let result = embed_resource::compile("../../tools/windows/yatype.rc", embed_resource::NONE);
    if cfg!(windows) {
        // На Windows компилятор ресурсов есть всегда — ошибка должна остановить сборку.
        result.manifest_required().expect("не удалось встроить иконку");
    } else if let Err(e) = result.manifest_optional() {
        // Проверка кода под Windows с другой системы: иконку не встроить, это не страшно.
        println!("cargo:warning=иконка не встроена: {e}");
    }
}
