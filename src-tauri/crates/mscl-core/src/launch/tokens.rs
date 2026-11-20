//! 占位符（${...}）替换（阶段 3）
//!
//! Mojang 的 arguments 里用 ${name} 占位符，需要替换为实际值。
//! 设计要点：
//!   1. 替换后校验：若还有未替换的 ${} 残留，返回错误（防止静默启动失败）
//!   2. 提供哪些 token 由调用方决定，常见 token 用常量名集中管理

use std::collections::HashMap;

use crate::error::CoreError;

// 常见占位符名称常量（集中管理，避免拼写错误）
pub const TOKEN_NATIVES_DIRECTORY: &str = "natives_directory";
pub const TOKEN_LAUNCHER_NAME: &str = "launcher_name";
pub const TOKEN_LAUNCHER_VERSION: &str = "launcher_version";
pub const TOKEN_CLASSPATH: &str = "classpath";
pub const TOKEN_AUTH_PLAYER_NAME: &str = "auth_player_name";
pub const TOKEN_VERSION_NAME: &str = "version_name";
pub const TOKEN_GAME_DIRECTORY: &str = "game_directory";
pub const TOKEN_ASSETS_ROOT: &str = "assets_root";
pub const TOKEN_ASSETS_INDEX_NAME: &str = "assets_index_name";
pub const TOKEN_AUTH_UUID: &str = "auth_uuid";
pub const TOKEN_AUTH_ACCESS_TOKEN: &str = "auth_access_token";
pub const TOKEN_CLIENT_ID: &str = "clientid";
pub const TOKEN_AUTH_XUID: &str = "auth_xuid";
pub const TOKEN_VERSION_TYPE: &str = "version_type";
pub const TOKEN_USER_TYPE: &str = "user_type";

/// Token 集合类型
pub type Tokens = HashMap<String, String>;

/// 占位符解析器
#[derive(Debug, Clone, Default)]
pub struct TokenResolver {
    tokens: Tokens,
}

impl TokenResolver {
    pub fn new(tokens: Tokens) -> Self {
        Self { tokens }
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.tokens.insert(key.to_string(), value.into());
    }

    /// 替换字符串中的所有 ${xxx} 占位符
    /// 返回 Err 若替换后仍有未解析的占位符
    pub fn resolve(&self, input: &str) -> Result<String, CoreError> {
        let mut result = input.to_string();
        for (key, value) in &self.tokens {
            let placeholder = format!("${{{}}}", key);
            result = result.replace(&placeholder, value);
        }

        // 校验残留占位符
        if let Some(unresolved) = find_placeholder(&result) {
            return Err(CoreError::Parse(format!(
                "存在未解析的占位符 ${{{}}}, 原始内容: {}",
                unresolved, input
            )));
        }
        Ok(result)
    }

    /// 替换一组参数（Vec<String>）
    pub fn resolve_all(&self, args: &[String]) -> Result<Vec<String>, CoreError> {
        args.iter().map(|a| self.resolve(a)).collect()
    }
}

/// 找出字符串中第一个 ${...} 占位符的名称
pub fn find_placeholder(s: &str) -> Option<String> {
    let start = s.find("${")?;
    let rest = &s[start + 2..];
    let end = rest.find('}')?;
    Some(rest[..end].to_string())
}

/// 判断参数是否含占位符
pub fn has_placeholder(s: &str) -> bool {
    s.contains("${")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_all_placeholders() {
        let mut tokens = Tokens::new();
        tokens.insert("name".into(), "MSCL".into());
        tokens.insert("version".into(), "26.3".into());
        let resolver = TokenResolver::new(tokens);

        assert_eq!(
            resolver.resolve("-Dbrand=${name}-${version}").unwrap(),
            "-Dbrand=MSCL-26.3"
        );
    }

    #[test]
    fn detects_unresolved_placeholder() {
        let mut tokens = Tokens::new();
        tokens.insert("name".into(), "MSCL".into());
        let resolver = TokenResolver::new(tokens);

        let result = resolver.resolve("${name}-${missing}");
        assert!(matches!(result, Err(CoreError::Parse(_))));
    }

    #[test]
    fn find_placeholder_works() {
        assert_eq!(find_placeholder("a${foo}b"), Some("foo".into()));
        assert_eq!(find_placeholder("no placeholder"), None);
    }

    #[test]
    fn resolves_classpath_with_semicolons() {
        let mut tokens = Tokens::new();
        tokens.insert("classpath".into(), "a.jar;b.jar;c.jar".into());
        let resolver = TokenResolver::new(tokens);
        assert_eq!(resolver.resolve("${classpath}").unwrap(), "a.jar;b.jar;c.jar");
    }
}
