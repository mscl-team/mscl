//! Rules 过滤逻辑（阶段 1 核心业务规则）
//!
//! Mojang 官方规则语义：
//!   1. 逐条评估 rules；空 rules 表示"适用所有平台"
//!   2. 每条 rule：若 os 条件匹配，则采用其 action（allow/disallow）
//!   3. 多条 rule 按顺序评估，后面的覆盖前面的结果
//!   4. features 条件：阶段 1 不判定（视为满足），因为启动器暂不提供可选特性
//!
//! 参考：https://minecraft.wiki/w/.minecraft/versions
//!
//! 平台用 core 内部定义的 TargetPlatform 表示，保持 core 层纯粹：
//! platform 层探测到系统后，转换为 TargetPlatform 传入。

use crate::model::library::{OsRule, Rule, RuleAction};

/// core 层的平台抽象（与具体 OS 探测解耦）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPlatform {
    Windows,
    MacOs,
    Linux,
    Android,
}

impl TargetPlatform {
    /// Mojang rules 中 os.name 的取值
    pub fn os_name(&self) -> &'static str {
        match self {
            TargetPlatform::Windows => "windows",
            TargetPlatform::MacOs => "osx",
            TargetPlatform::Linux => "linux",
            TargetPlatform::Android => "linux", // Android 复用 linux 规则
        }
    }

    /// 当前架构标识（Mojang 用 x86 表示 32 位，x64 用 none 表示）
    pub fn arch(&self) -> &'static str {
        "x64"
    }
}

/// 判断 OsRule 是否匹配目标平台
/// 规则：os 中每个指定字段都必须匹配（AND 关系）；未指定的字段视为通配
pub fn os_rule_matches(os: &OsRule, platform: TargetPlatform) -> bool {
    if let Some(name) = &os.name {
        if name != platform.os_name() {
            return false;
        }
    }
    if let Some(arch) = &os.arch {
        // Mojang 的 arch 值：x86 表示 32 位。当前启动器面向 64 位，
        // 因此 x86 条件不匹配；其他值（如 arm64）暂按不匹配处理，避免误判。
        if arch != platform.arch() {
            return false;
        }
    }
    if let Some(version) = &os.version {
        // os.version 是正则，阶段 1 不做版本正则匹配，视为通配
        let _ = version;
    }
    true
}

/// 评估单条 rule 是否"命中"目标平台
/// 返回值含义：该 rule 是否对当前平台生效
pub fn rule_allows(rule: &Rule, platform: TargetPlatform) -> bool {
    match &rule.os {
        Some(os) => {
            if os_rule_matches(os, platform) {
                // os 匹配 → 按 action 决定
                rule.action == RuleAction::Allow
            } else {
                // os 不匹配 → 这条 rule 不生效。
                // 对于 allow 规则，不匹配意味着"不因此允许"；后续规则继续评估
                false
            }
        }
        None => {
            // 无 os 条件（只有 action，或有 features）
            // features 阶段 1 视为满足；无 os 即对所有平台生效
            rule.action == RuleAction::Allow
        }
    }
}

/// 评估一组 rules 的最终结果（供整个 library 使用）
/// 语义：初始为"不允许"，逐条匹配，命中则更新结果（后者覆盖前者）
pub fn evaluate_rules(rules: &[Rule], platform: TargetPlatform) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(os) => os_rule_matches(os, platform),
            None => true, // features 视为满足
        };
        if os_match {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::library::OsRule;

    fn rule(action: RuleAction, os: Option<OsRule>) -> Rule {
        Rule { action, os, features: None }
    }

    #[test]
    fn empty_rules_allow_all() {
        assert!(evaluate_rules(&[], TargetPlatform::Windows));
        assert!(evaluate_rules(&[], TargetPlatform::MacOs));
    }

    #[test]
    fn allow_windows_only() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsRule { name: Some("windows".into()), ..Default::default() }),
        )];
        assert!(evaluate_rules(&rules, TargetPlatform::Windows));
        assert!(!evaluate_rules(&rules, TargetPlatform::MacOs));
        assert!(!evaluate_rules(&rules, TargetPlatform::Linux));
    }

    #[test]
    fn disallow_osx_allows_others() {
        // 典型模式：先 allow 全部，再 disallow osx
        let rules = vec![
            rule(RuleAction::Allow, None),
            rule(
                RuleAction::Disallow,
                Some(OsRule { name: Some("osx".into()), ..Default::default() }),
            ),
        ];
        assert!(evaluate_rules(&rules, TargetPlatform::Windows));
        assert!(!evaluate_rules(&rules, TargetPlatform::MacOs));
        assert!(evaluate_rules(&rules, TargetPlatform::Linux));
    }

    #[test]
    fn later_rule_overrides_earlier() {
        // allow windows, then disallow windows → 最终 disallow
        let rules = vec![
            rule(
                RuleAction::Allow,
                Some(OsRule { name: Some("windows".into()), ..Default::default() }),
            ),
            rule(
                RuleAction::Disallow,
                Some(OsRule { name: Some("windows".into()), ..Default::default() }),
            ),
        ];
        assert!(!evaluate_rules(&rules, TargetPlatform::Windows));
    }

    #[test]
    fn arch_x86_does_not_match_x64() {
        let os = OsRule { name: Some("windows".into()), arch: Some("x86".into()), version: None };
        assert!(!os_rule_matches(&os, TargetPlatform::Windows));
    }

    #[test]
    fn android_reuses_linux_rules() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsRule { name: Some("linux".into()), ..Default::default() }),
        )];
        assert!(evaluate_rules(&rules, TargetPlatform::Android));
    }
}
