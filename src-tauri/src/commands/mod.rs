//! Tauri Commands 实现
//!
//! 把 core + platform 能力暴露为前端可调用的接口。
//! 每个 command 都是薄薄一层：参数转换 → 调用 core/platform → 结果转 DTO。

mod account;
pub mod backup;
mod game;
mod instance;
pub mod isolation;
mod maintenance;
mod modpack;
pub mod plugin;
mod resource;
pub mod targets;
mod skin;
mod system;
pub mod updates;

pub use account::{
    authenticate_external, delete_account, get_active_account, list_accounts, set_active_account,
    upsert_offline_account,
};
pub use instance::{
    cancel_instance_install, create_instance, delete_instance, delete_instance_resource,
    import_local_resource, list_instance_resources, list_instances, move_instance_resource,
    pause_instance_install, resume_instance_install, update_instance_meta,
};
pub use maintenance::{
    add_game_root, clear_cache, clear_log_files, delete_game_data, get_game_dir_info,
    get_log_dir_info, list_all_instances, list_game_roots, open_external_url, open_log_dir,
    remove_game_root, reset_log_dir, reset_settings, set_game_dir, set_log_dir,
};
pub use modpack::import_mrpack;
pub use backup::{
    create_launcher_backup, delete_launcher_backup, list_launcher_backups,
    restore_launcher_backup,
};
pub use plugin::{
    check_plugin_repo_updates, check_plugin_update, export_plugin_configs,
    get_default_plugin_config, get_plugin_config, get_plugin_manifest, import_plugin_configs,
    install_builtin_plugin, install_github_plugin, install_local_plugin, list_installed_plugins,
    read_plugin_manifest_from_dir, set_plugin_config, set_plugin_enabled, uninstall_plugin,
    update_plugin_repo_safe, validate_plugin_manifest, write_default_plugin_config,
    write_plugin_export_file, import_plugin_configs_from_file,
};
pub use resource::{
    check_resource_install, detect_resource_file, get_resource_detail, import_mrpack_url,
    install_resource, list_resource_versions, search_resources,
};
pub use skin::{
    download_cape, download_skin, fetch_skin_profile, import_cape_file, import_skin_file,
    list_local_skins, save_cape_data, save_skin_data,
};
pub use game::{
    clear_game_pid, delete_mod, get_installed_versions, get_instance_launch_options,
    get_running_instances, get_runtime_metrics, install_fabric, install_forge, install_game, launch_game, list_mods,
    open_instance_dir, repair_version, save_instance_launch_options, stop_game, stop_run,
    stop_version, toggle_mod, uninstall_version, verify_version,
};
pub use system::{
    backend_status, detect_java, export_settings, get_instance_isolation, get_isolation_overview,
    get_settings, get_uninstall_info, get_version_list, import_settings, read_settings_file,
    save_settings, save_settings_file, uninstall_launcher, window_control,
};
pub use targets::{add_resource_target_dir, list_resource_targets, remove_resource_target_dir};
pub use updates::{check_launcher_updates, check_startup_updates, prepare_launcher_update};
