//! 启动参数解析（阶段 3）
//!
//! Mojang 的 arguments.jvm / arguments.game 是混合数组：
//!   - 字符串：无条件加入，如 "-Xmx2G"
//!   - 对象：{"rules": [...], "value": "xxx" 或 ["a","b"]}
//!       * rules 语义：任一 rule 命中（os 匹配且 allow）则加入该 value
//!       * 若 rule 带 features（如 is_demo_user），我们未启用该特性 → 不加入
//!       * 注意：这与 library 的"逐条覆盖"语义不同！
//!
//! 旧版本（1.12-）用 minecraftArguments 字符串，按空格切分即可（无复杂转义）。

use serde_json::Value;

use crate::model::Version;
use crate::rules::{os_rule_matches, TargetPlatform};

/// 把 arguments 数组解析为"扁平的参数列表"（尚未替换占位符）
/// 只保留当前平台适用、且不依赖未启用 feature 的参数
pub fn resolve_argument_array(
    args: &[Value],
    platform: TargetPlatform,
) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Value::String(s) => out.push(s.clone()),
            Value::Object(obj) => {
                // 检查 rules
                let rules = obj.get("rules").and_then(|v| v.as_array());
                let allowed = match rules {
                    None => true,
                    Some(rules) => argument_rules_allow(rules, platform),
                };
                if !allowed {
                    continue;
                }
                // 展开 value（字符串或字符串数组）
                if let Some(value) = obj.get("value") {
                    match value {
                        Value::String(s) => out.push(s.clone()),
                        Value::Array(arr) => {
                            for v in arr {
                                if let Some(s) = v.as_str() {
                                    out.push(s.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// arguments 里的 rules 判定：任一 rule 命中则允许
/// rule 命中 = action 为 allow 且 os 条件匹配
/// 带 features 的 rule：因未启用任何可选特性，视为不命中
fn argument_rules_allow(rules: &[Value], platform: TargetPlatform) -> bool {
    for rule in rules {
        let obj = match rule.as_object() {
            Some(o) => o,
            None => continue,
        };
        let action = obj.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if action != "allow" {
            continue;
        }

        // 带 features 的规则：未启用对应特性，跳过
        if obj.contains_key("features") {
            continue;
        }

        // os 条件
        match obj.get("os") {
            None => return true, // 无 os 条件的 allow → 适用所有平台
            Some(os_val) => {
                if let Some(os_obj) = os_val.as_object() {
                    let os_rule = crate::model::OsRule {
                        name: os_obj.get("name").and_then(|v| v.as_str()).map(String::from),
                        arch: os_obj.get("arch").and_then(|v| v.as_str()).map(String::from),
                        version: os_obj.get("version").and_then(|v| v.as_str()).map(String::from),
                    };
                    if os_rule_matches(&os_rule, platform) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// 获取某个版本的 jvm 参数列表（扁平化，未替换占位符）
pub fn jvm_arguments(version: &Version, platform: TargetPlatform) -> Vec<String> {
    match &version.arguments {
        Some(args) => resolve_argument_array(&args.jvm, platform),
        None => {
            // 旧版本没有 arguments.jvm，用经典的默认 JVM 参数
            vec![
                "-Djava.library.path=${natives_directory}".to_string(),
                "-cp".to_string(),
                "${classpath}".to_string(),
            ]
        }
    }
}

/// 获取某个版本的 game 参数列表（扁平化，未替换占位符）
pub fn game_arguments(version: &Version, platform: TargetPlatform) -> Vec<String> {
    match &version.arguments {
        Some(args) => resolve_argument_array(&args.game, platform),
        None => {
            // 旧版本用 minecraftArguments 字符串，按空白切分
            version
                .minecraft_arguments
                .as_ref()
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_strings_kept() {
        let args = vec![json!("-Xmx2G"), json!("-XX:+UseG1GC")];
        assert_eq!(resolve_argument_array(&args, TargetPlatform::Windows),
            vec!["-Xmx2G", "-XX:+UseG1GC"]);
    }

    #[test]
    fn os_specific_arg_filtered() {
        let args = vec![
            json!({"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": "-XstartOnFirstThread"}),
            json!("-Xmx2G"),
        ];
        let mac = resolve_argument_array(&args, TargetPlatform::MacOs);
        let win = resolve_argument_array(&args, TargetPlatform::Windows);
        assert_eq!(mac, vec!["-XstartOnFirstThread", "-Xmx2G"]);
        assert_eq!(win, vec!["-Xmx2G"]);
    }

    #[test]
    fn feature_arguments_excluded() {
        let args = vec![
            json!({"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}),
            json!("--username"),
        ];
        let resolved = resolve_argument_array(&args, TargetPlatform::Windows);
        assert_eq!(resolved, vec!["--username"], "feature 参数应被排除");
    }

    #[test]
    fn array_value_expanded() {
        let args = vec![json!({
            "rules": [{"action": "allow", "os": {"name": "windows"}}],
            "value": ["-Dos.name=Windows 10", "-Dos.version=10.0"]
        })];
        let resolved = resolve_argument_array(&args, TargetPlatform::Windows);
        assert_eq!(resolved, vec!["-Dos.name=Windows 10", "-Dos.version=10.0"]);
    }
}
