// 在 Windows 上 release 构建时不显示控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 更新助手：主窗口关闭后替换插件库或启动器本体，失败由助手自行回滚
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--apply-plugin-update") {
        let archive = args.get(pos + 1).cloned().unwrap_or_default();
        let backup = args.get(pos + 2).cloned().unwrap_or_default();
        let root = args.get(pos + 3).cloned().unwrap_or_default();
        if !archive.is_empty() && !backup.is_empty() && !root.is_empty() {
            let result = mscl_app_lib::apply_plugin_update(
                std::path::Path::new(&archive),
                std::path::Path::new(&backup),
                std::path::Path::new(&root),
            );
            match result {
                Ok(()) => println!("PLUGIN_UPDATE_OK"),
                Err(e) => {
                    let log = std::path::Path::new(&backup)
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .join("plugin-update-error.log");
                    let _ = std::fs::write(log, e.clone());
                    eprintln!("PLUGIN_UPDATE_ROLLED_BACK {}", e);
                }
            }
        }
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--apply-launcher-update") {
        let download = args.get(pos + 1).cloned().unwrap_or_default();
        if !download.is_empty() {
            match mscl_app_lib::apply_launcher_update(std::path::Path::new(&download)) {
                Ok(()) => println!("LAUNCHER_UPDATE_OK"),
                Err(e) => {
                    let log = std::path::Path::new(&download)
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .join("launcher-update-error.log");
                    let _ = std::fs::write(log, e.clone());
                    eprintln!("LAUNCHER_UPDATE_ROLLED_BACK {}", e);
                }
            }
        }
        return;
    }

    mscl_app_lib::run();
}
