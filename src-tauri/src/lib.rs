//! MSCL Tauri 应用层入口
//!
//! 桥接层：把 core 与 platform 的能力，以类型安全的 Tauri command / event 暴露给前端。

mod commands;
mod models;
mod state;

pub use commands::plugin::apply_plugin_update;
pub use commands::updates::apply_launcher_update;

use tauri::Manager;

use commands::{
    add_game_root, add_resource_target_dir, authenticate_external, backend_status,
    cancel_instance_install, check_launcher_updates, check_plugin_repo_updates, check_plugin_update,
    check_resource_install, check_startup_updates, clear_cache, create_launcher_backup,
    clear_game_pid, clear_log_files, create_instance,
    delete_account, delete_game_data, delete_instance, delete_instance_resource, delete_launcher_backup, delete_mod,
    detect_java, detect_resource_file, download_cape, download_skin, export_plugin_configs,
    export_settings, fetch_skin_profile, get_active_account, get_default_plugin_config,
    get_plugin_config, get_plugin_manifest, get_game_dir_info, get_installed_versions, get_instance_launch_options, get_log_dir_info,
    get_resource_detail, get_running_instances, get_runtime_metrics, get_instance_isolation,
    get_isolation_overview, get_settings, get_uninstall_info, get_version_list,
    import_cape_file, import_mrpack, import_mrpack_url, import_settings, import_skin_file,
    install_fabric, install_forge, install_game, install_github_plugin, import_local_resource,
    install_builtin_plugin, install_local_plugin, install_resource, launch_game, list_accounts, list_all_instances,
    list_game_roots, list_instance_resources, list_instances, list_installed_plugins,
    list_launcher_backups, list_local_skins, list_mods, list_resource_versions, list_resource_targets,
    move_instance_resource, open_external_url, open_instance_dir, open_log_dir,
    pause_instance_install, prepare_launcher_update, remove_game_root, remove_resource_target_dir,
    repair_version, reset_log_dir, reset_settings,
    read_settings_file, read_plugin_manifest_from_dir, resume_instance_install, restore_launcher_backup, save_cape_data,
    save_instance_launch_options, save_settings, save_settings_file, save_skin_data,
    search_resources, set_active_account, set_game_dir, set_log_dir, set_plugin_config,
    set_plugin_enabled, stop_game, stop_run, stop_version, toggle_mod, uninstall_launcher,
    uninstall_plugin, uninstall_version, update_instance_meta, update_plugin_repo_safe,
    upsert_offline_account, validate_plugin_manifest, verify_version, window_control,
    write_default_plugin_config, write_plugin_export_file, import_plugin_configs,
    import_plugin_configs_from_file,
};
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // 更新统一走 GitHub Release + 自定义安全更新助手（见 commands/updates.rs），
        // 不启用 Tauri 内置 updater，避免两套机制并存与假端点误触发。
        .plugin(tauri_plugin_process::init())
        // 注入全局状态（HTTP client + 游戏目录 + 运行进程 PID）
        .setup(|app| {
            let state = AppState::new().map_err(|e| e.to_string())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 保留 greet 兼容阶段 0 验证
            greet,
            // 系统类
            backend_status,
            get_version_list,
            detect_java,
            window_control,
            get_uninstall_info,
            uninstall_launcher,
            // 插件系统
            list_installed_plugins,
            get_plugin_manifest,
            get_default_plugin_config,
            get_plugin_config,
            set_plugin_config,
            set_plugin_enabled,
            export_plugin_configs,
            import_plugin_configs,
            check_plugin_update,
            validate_plugin_manifest,
            read_plugin_manifest_from_dir,
            write_plugin_export_file,
            import_plugin_configs_from_file,
            install_local_plugin,
            install_github_plugin,
            uninstall_plugin,
            check_plugin_repo_updates,
            update_plugin_repo_safe,
            write_default_plugin_config,
            install_builtin_plugin,
            // 更新与备份
            check_startup_updates,
            check_launcher_updates,
            prepare_launcher_update,
            create_launcher_backup,
            list_launcher_backups,
            restore_launcher_backup,
            delete_launcher_backup,
            // 资源安装目标
            list_resource_targets,
            add_resource_target_dir,
            remove_resource_target_dir,
            get_settings,
            save_settings,
            export_settings,
            import_settings,
            save_settings_file,
            read_settings_file,
            get_instance_isolation,
            get_isolation_overview,
            list_accounts,
            get_active_account,
            upsert_offline_account,
            authenticate_external,
            set_active_account,
            delete_account,
            // 游戏类
            install_game,
            launch_game,
            get_running_instances,
            get_runtime_metrics,
            stop_run,
            stop_version,
            get_instance_launch_options,
            save_instance_launch_options,
            get_installed_versions,
            stop_game,
            uninstall_version,
            clear_game_pid,
            verify_version,
            repair_version,
            list_mods,
            toggle_mod,
            delete_mod,
            open_instance_dir,
            install_fabric,
            install_forge,
            import_mrpack,
            import_mrpack_url,
            create_instance,
            cancel_instance_install,
            pause_instance_install,
            resume_instance_install,
            list_instances,
            list_all_instances,
            list_game_roots,
            add_game_root,
            remove_game_root,
            search_resources,
            install_resource,
            check_resource_install,
            update_instance_meta,
            list_instance_resources,
            delete_instance_resource,
            move_instance_resource,
            import_local_resource,
            list_resource_versions,
            fetch_skin_profile,
            download_skin,
            download_cape,
            save_skin_data,
            save_cape_data,
            import_skin_file,
            import_cape_file,
            list_local_skins,
            get_game_dir_info,
            set_game_dir,
            reset_settings,
            delete_game_data,
            open_external_url,
            delete_instance,
            get_resource_detail,
            detect_resource_file,
            get_log_dir_info,
            set_log_dir,
            reset_log_dir,
            open_log_dir,
            clear_log_files,
            clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("MSCL 应用启动失败");
}

/// 兼容阶段 0 的 greet 命令（前端验证 IPC 用）
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! MSCL 后端已就绪。", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_works() {
        assert!(greet("MSCL").contains("MSCL"));
    }
}
