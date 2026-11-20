//! 启动模块（阶段 3）
//!
//! - tokens.rs: ${} 占位符替换
//! - args.rs: JVM/game 参数按平台与 rules 解析
//! - classpath.rs: classpath 拼接（平台分隔符）
//! - command.rs: 组装完整启动命令

mod args;
mod classpath;
mod command;
mod tokens;

pub use args::{game_arguments, jvm_arguments, resolve_argument_array};
pub use classpath::{build_classpath, classpath_separator};
pub use command::{
    build_launch_command, Account, LaunchCommand, LaunchConfig,
};
pub use tokens::{
    find_placeholder, has_placeholder, TokenResolver, Tokens, TOKEN_ASSETS_INDEX_NAME,
    TOKEN_ASSETS_ROOT, TOKEN_AUTH_ACCESS_TOKEN, TOKEN_AUTH_PLAYER_NAME, TOKEN_AUTH_UUID,
    TOKEN_AUTH_XUID, TOKEN_CLASSPATH, TOKEN_CLIENT_ID, TOKEN_GAME_DIRECTORY, TOKEN_LAUNCHER_NAME,
    TOKEN_LAUNCHER_VERSION, TOKEN_NATIVES_DIRECTORY, TOKEN_USER_TYPE, TOKEN_VERSION_NAME,
    TOKEN_VERSION_TYPE,
};
