//! 版本继承合并（阶段 10）
//!
//! Fabric / Forge 等加载器的版本 JSON 通过 `inheritsFrom` 指向原版，
//! 自身只声明额外的 libraries、mainClass 与参数。
//! 启动与安装前必须合并成完整版本，否则会缺失原版库与资源索引。
//!
//! 合并规则（与主流启动器一致）：
//!   - id / mainClass / arguments：子版本优先，父版本兜底
//!   - arguments：父在前、子在后拼接（两段都保留）
//!   - libraries：父在前、子在后，按 group:artifact 去重（子覆盖父）
//!   - downloads：子优先，缺失项沿用父版本
//!   - client.jar 始终来自最初的父版本（原版）

use super::version::VersionDownloads;
use super::{Library, Version, VersionArguments};

/// 合并加载器版本与父版本
pub fn merge_inherited(child: &Version, parent: &Version) -> Version {
    // libraries：父在前、子在后，子同坐标覆盖父
    let mut libraries: Vec<Library> = parent.libraries.clone();
    for lib in &child.libraries {
        match libraries.iter().position(|l| same_coord(&l.name, &lib.name)) {
            Some(pos) => libraries[pos] = lib.clone(),
            None => libraries.push(lib.clone()),
        }
    }

    // arguments：父在前、子在后
    let arguments = match (&child.arguments, &parent.arguments) {
        (Some(c), Some(p)) => Some(VersionArguments {
            game: [p.game.clone(), c.game.clone()].concat(),
            jvm: [p.jvm.clone(), c.jvm.clone()].concat(),
        }),
        (Some(c), None) => Some(c.clone()),
        (None, Some(p)) => Some(p.clone()),
        (None, None) => None,
    };

    // downloads：父的作为底，子的覆盖
    let mut downloads: VersionDownloads = parent.downloads.clone();
    for (key, value) in &child.downloads {
        downloads.insert(key.clone(), value.clone());
    }

    Version {
        id: child.id.clone(),
        version_type: child.version_type,
        main_class: if child.main_class.is_empty() {
            parent.main_class.clone()
        } else {
            child.main_class.clone()
        },
        arguments,
        minecraft_arguments: child
            .minecraft_arguments
            .clone()
            .or_else(|| parent.minecraft_arguments.clone()),
        downloads,
        asset_index: child
            .asset_index
            .clone()
            .or_else(|| parent.asset_index.clone()),
        assets: child.assets.clone().or_else(|| parent.assets.clone()),
        libraries,
        java_version: child
            .java_version
            .clone()
            .or_else(|| parent.java_version.clone()),
        // Forge 会生成 `"logging": {}`（client 为 None），这种情况下应继承父版本
        logging: child
            .logging
            .clone()
            .filter(|cfg| cfg.client.is_some())
            .or_else(|| parent.logging.clone()),
        minimum_launcher_version: child
            .minimum_launcher_version
            .or(parent.minimum_launcher_version),
        compliance_level: child.compliance_level.or(parent.compliance_level),
        inherits_from: None,
        // client.jar 来自最初的父版本；多层继承时沿用父版本已记录的值
        client_jar_from: child
            .client_jar_from
            .clone()
            .or_else(|| Some(parent.id.clone())),
    }
}

/// 判断两个 Maven 坐标是否指向同一组件（比较 group:artifact，忽略 version）
fn same_coord(a: &str, b: &str) -> bool {
    fn key(s: &str) -> String {
        s.split(':').take(2).collect::<Vec<_>>().join(":")
    }
    key(a) == key(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent() -> Version {
        serde_json::from_str(
            r#"{
                "id": "1.21.1",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "arguments": {
                    "game": ["--username", "${auth_player_name}"],
                    "jvm": ["-Djava.library.path=${natives_directory}"]
                },
                "downloads": {
                    "client": {"path": "client.jar", "sha1": "c1", "size": 100, "url": "https://piston-data.mojang.com/client.jar"}
                },
                "assetIndex": {"id": "17", "sha1": "i1", "size": 10, "url": "https://piston-meta.mojang.com/index.json"},
                "assets": "17",
                "libraries": [
                    {"name": "org.lwjgl:lwjgl:3.3.3", "downloads": {"artifact": {"path": "org/lwjgl/lwjgl.jar", "sha1": "l1", "size": 10, "url": "u1"}}}
                ],
                "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21}
            }"#,
        )
        .unwrap()
    }

    fn fabric_child() -> Version {
        serde_json::from_str(
            r#"{
                "id": "fabric-loader-0.16.9-1.21.1",
                "inheritsFrom": "1.21.1",
                "type": "release",
                "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                "arguments": {"game": ["--fabric-side"], "jvm": []},
                "libraries": [
                    {"name": "net.fabricmc:fabric-loader:0.16.9", "downloads": {"artifact": {"path": "net/fabricmc/fabric-loader.jar", "sha1": "f1", "size": 20, "url": "u2"}}},
                    {"name": "net.fabricmc:intermediary:1.21.1", "downloads": {"artifact": {"path": "net/fabricmc/intermediary.jar", "sha1": "f2", "size": 30, "url": "u3"}}}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn merge_keeps_child_identity_and_parent_data() {
        let merged = merge_inherited(&fabric_child(), &parent());

        assert_eq!(merged.id, "fabric-loader-0.16.9-1.21.1");
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(merged.inherits_from, None);
        assert_eq!(merged.client_jar_from.as_deref(), Some("1.21.1"));
        // 资源索引 / Java 版本从父版本继承
        assert_eq!(merged.asset_index.unwrap().id, "17");
        assert_eq!(merged.java_version.unwrap().major_version, 21);
        // client 下载项来自父版本
        assert!(merged.downloads.contains_key("client"));
    }

    #[test]
    fn libraries_are_appended_in_order() {
        let merged = merge_inherited(&fabric_child(), &parent());
        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "org.lwjgl:lwjgl:3.3.3",
                "net.fabricmc:fabric-loader:0.16.9",
                "net.fabricmc:intermediary:1.21.1",
            ]
        );
    }

    #[test]
    fn child_library_overrides_same_coordinate() {
        let mut child = fabric_child();
        // 子版本用不同版本号覆盖同一个 group:artifact
        child.libraries.push(
            serde_json::from_str(
                r#"{"name": "org.lwjgl:lwjgl:9.9.9", "downloads": {"artifact": {"path": "org/lwjgl/lwjgl-new.jar", "sha1": "new", "size": 1, "url": "u"}}}"#,
            )
            .unwrap(),
        );

        let merged = merge_inherited(&child, &parent());
        let lwjgl = merged
            .libraries
            .iter()
            .find(|l| l.name.starts_with("org.lwjgl:lwjgl:"))
            .unwrap();
        assert_eq!(lwjgl.name, "org.lwjgl:lwjgl:9.9.9");
        // 不应出现两个 lwjgl
        assert_eq!(
            merged
                .libraries
                .iter()
                .filter(|l| l.name.starts_with("org.lwjgl:lwjgl:"))
                .count(),
            1
        );
    }

    #[test]
    fn arguments_parent_then_child() {
        let merged = merge_inherited(&fabric_child(), &parent());
        let args = merged.arguments.unwrap();
        let game: Vec<String> = args
            .game
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        assert_eq!(game, vec!["--username", "${auth_player_name}", "--fabric-side"]);
    }

    #[test]
    fn plan_client_uses_parent_version_dir() {
        let merged = merge_inherited(&fabric_child(), &parent());
        let task = crate::download::DownloadTask::plan_client(&merged).unwrap();
        // client.jar 应指向父版本（1.21.1）目录，而不是 fabric 版本目录
        assert_eq!(task.relative_path, "versions/1.21.1/1.21.1.jar");
    }
}
