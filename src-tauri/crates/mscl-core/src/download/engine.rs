//! 并发下载引擎（阶段 2）
//!
//! 职责：消费 DownloadTask 列表，完成实际的网络下载 + 落盘 + 校验。
//! 关键设计：
//!   1. 并发控制：futures::buffer_unordered 限制同时下载数，避免打爆带宽/句柄
//!   2. 校验跳过：下载前先 verify，Valid 则跳过（缓存命中）
//!   3. 失败重试：每个任务最多重试 max_retries 次，指数退避
//!   4. 下载后校验：落盘后立即 SHA1+size 校验，损坏则删除
//!   5. 进度事件：通过 ProgressCallback 推送，桥接层转为 Tauri event

use futures::stream::{self, StreamExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::source::DownloadSource;
use super::task::DownloadTask;
use super::verify::{verify_file, VerifyOutcome};
use crate::error::CoreError;

/// 单个任务的下载结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskResult {
    /// 本次实际下载完成
    Downloaded,
    /// 已存在且校验通过，跳过
    Skipped,
}

/// 进度事件（桥接层会把它转成 Tauri event 推给前端）
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// 开始一批任务
    Started { total: usize, to_download: usize },
    /// 单个任务完成（含跳过）
    TaskDone {
        path: String,
        result: TaskResult,
        completed: usize,
        total: usize,
    },
    /// 全部完成
    Finished { downloaded: usize, skipped: usize, failed: usize },
}

/// 进度回调类型
pub type ProgressCallback = Arc<dyn Fn(DownloadEvent) + Send + Sync>;

/// 下载引擎配置
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// 并发下载数
    pub concurrency: usize,
    /// 单个任务最大重试次数
    pub max_retries: u32,
    /// 重试基础延迟毫秒（指数退避：delay * 2^(retry-1)）
    pub retry_base_delay_ms: u64,
    /// 单文件下载超时秒数（0 表示不额外限制，依赖 HTTP client 自身超时）
    pub timeout_secs: u64,
    /// 镜像失败重试时是否回退官方源（提升镜像不可用时的成功率）
    pub fallback_to_official: bool,
    /// 用户中断标记：置位后当前批次尽快停止（已写出的临时文件会被丢弃）
    pub cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            concurrency: 8,
            max_retries: 3,
            retry_base_delay_ms: 500,
            timeout_secs: 120,
            fallback_to_official: true,
            cancel: None,
        }
    }
}

/// 下载引擎
#[derive(Clone)]
pub struct DownloadEngine {
    client: reqwest::Client,
    /// 游戏根目录，任务的 relative_path 都相对它
    root: PathBuf,
    options: DownloadOptions,
    /// 下载源（官方 / 镜像 / 自定义）
    source: DownloadSource,
}

impl DownloadEngine {
    pub fn new(client: reqwest::Client, root: PathBuf) -> Self {
        Self {
            client,
            root,
            options: DownloadOptions::default(),
            source: DownloadSource::Official,
        }
    }

    pub fn with_options(mut self, options: DownloadOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_source(mut self, source: DownloadSource) -> Self {
        self.source = source;
        self
    }

    /// 当前下载源（供桥接层提示用）
    pub fn source(&self) -> &DownloadSource {
        &self.source
    }

    /// 用户是否已请求中断
    fn cancelled(&self) -> bool {
        self.options
            .cancel
            .as_ref()
            .map(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// 下载所有任务
    pub async fn download_all(
        &self,
        tasks: &[DownloadTask],
        on_progress: Option<ProgressCallback>,
    ) -> Result<DownloadSummary, CoreError> {
        // 阶段 1：先同步分流——已校验通过的跳过，其余需要下载
        // 用 owned DownloadTask（clone），避免 stream 迭代器的 item 带借用生命周期，
        // 这样返回的 future 才是 lifetime-generic（Tauri async command / tokio::spawn 要求）
        let mut pending: Vec<DownloadTask> = Vec::new();
        let mut skipped = 0usize;

        for task in tasks {
            let full_path = self.root.join(&task.relative_path);
            match verify_file(&full_path, &task.sha1, task.size)? {
                VerifyOutcome::Valid => {
                    skipped += 1;
                    if let Some(cb) = &on_progress {
                        cb(DownloadEvent::TaskDone {
                            path: task.relative_path.clone(),
                            result: TaskResult::Skipped,
                            completed: 0,
                            total: tasks.len(),
                        });
                    }
                }
                _ => pending.push(task.clone()), // Missing 或 Corrupted 都要下载
            }
        }

        if let Some(cb) = &on_progress {
            cb(DownloadEvent::Started {
                total: tasks.len(),
                to_download: pending.len(),
            });
        }

        let total = tasks.len();
        let completed = std::sync::atomic::AtomicUsize::new(skipped);
        let failed_count = std::sync::atomic::AtomicUsize::new(0);
        let mut downloaded = 0usize;

        // 并发下载 pending 任务
        let mut results = stream::iter(pending)
            .map(|task| {
                let completed = &completed;
                let failed = &failed_count;
                let cb = on_progress.clone();
                async move {
                    if self.cancelled() {
                        return (
                            task,
                            Err(CoreError::Network("安装已取消".to_string())),
                        );
                    }
                    let result = self.download_one_with_retry(&task).await;
                    let is_ok = result.is_ok();
                    let n = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    if !is_ok {
                        failed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    if let Some(cb) = cb {
                        cb(DownloadEvent::TaskDone {
                            path: task.relative_path.clone(),
                            result: if is_ok { TaskResult::Downloaded } else { TaskResult::Skipped },
                            completed: n,
                            total,
                        });
                    }
                    (task, result)
                }
            })
            .buffer_unordered(self.options.concurrency);

        let mut errors: Vec<(String, CoreError)> = Vec::new();
        while let Some((task, result)) = results.next().await {
            match result {
                Ok(()) => downloaded += 1,
                Err(e) => errors.push((task.relative_path.clone(), e)),
            }
        }

        let failed = errors.len();
        if let Some(cb) = &on_progress {
            cb(DownloadEvent::Finished {
                downloaded,
                skipped,
                failed,
            });
        }

        // 只要有失败就汇总返回（调用方决定是否整体失败）
        if errors.is_empty() {
            Ok(DownloadSummary { downloaded, skipped, failed: 0, errors: vec![] })
        } else {
            Ok(DownloadSummary { downloaded, skipped, failed, errors })
        }
    }

    /// 单任务下载 + 重试
    ///
    /// 回退策略：首次用所选源；若镜像失败，后续重试改走官方源
    /// （镜像偶尔缺文件或临时故障时，仍然能把版本装完）。
    async fn download_one_with_retry(&self, task: &DownloadTask) -> Result<(), CoreError> {
        let mut last_err = None;
        for attempt in 0..=self.options.max_retries {
            if self.cancelled() {
                return Err(CoreError::Network("安装已取消".to_string()));
            }
            if attempt > 0 {
                let delay = self.options.retry_base_delay_ms * (1u64 << (attempt - 1).min(6));
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            let use_official =
                attempt > 0 && self.options.fallback_to_official && !self.source.is_official();

            let attempt_fut = self.download_one(task, use_official);
            let result = if self.options.timeout_secs > 0 {
                match tokio::time::timeout(
                    Duration::from_secs(self.options.timeout_secs),
                    attempt_fut,
                )
                .await
                {
                    Ok(r) => r,
                    Err(_) => Err(CoreError::Network(format!(
                        "下载超时（{} 秒）: {}",
                        self.options.timeout_secs, task.relative_path
                    ))),
                }
            } else {
                attempt_fut.await
            };

            match result {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| CoreError::Network("下载失败且无错误信息".into())))
    }

    /// 实际下载单个文件：流式写入 + 落盘校验
    ///
    /// `use_official` 为 true 时忽略当前源，直接用任务里的官方 URL。
    async fn download_one(
        &self,
        task: &DownloadTask,
        use_official: bool,
    ) -> Result<(), CoreError> {
        let full_path = self.root.join(&task.relative_path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                CoreError::Network(format!("创建目录失败 {}: {}", parent.display(), e))
            })?;
        }

        let url = if use_official {
            task.url.clone()
        } else {
            self.source.rewrite(&task.url)
        };

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("下载失败 {}: {}", url, e)))?;

        if !response.status().is_success() {
            return Err(CoreError::Network(format!(
                "下载 {} 返回状态 {}",
                url,
                response.status()
            )));
        }

        // 流式写入临时文件（防止下载中断留下损坏的正式文件）
        let tmp_path = full_path.with_extension(format!("part-{}", std::process::id()));
        {
            use futures::StreamExt;
            let mut stream = response.bytes_stream();
            let mut file = tokio::fs::File::create(&tmp_path).await.map_err(|e| {
                CoreError::Network(format!("创建临时文件失败 {}: {}", tmp_path.display(), e))
            })?;
            while let Some(chunk) = stream.next().await {
                if self.cancelled() {
                    return Err(CoreError::Network("安装已取消".into()));
                }
                let chunk = chunk.map_err(|e| CoreError::Network(format!("读取下载流失败: {}", e)))?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                    .await
                    .map_err(|e| CoreError::Network(format!("写入文件失败: {}", e)))?;
            }
        }

        // 校验临时文件
        match verify_file(&tmp_path, &task.sha1, task.size)? {
            VerifyOutcome::Valid => {
                tokio::fs::rename(&tmp_path, &full_path).await.map_err(|e| {
                    CoreError::Network(format!("重命名文件失败 {}: {}", full_path.display(), e))
                })?;
                Ok(())
            }
            other => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                Err(CoreError::Checksum(format!(
                    "下载 {} 校验失败: {:?}",
                    task.relative_path, other
                )))
            }
        }
    }

    /// 供测试用：校验某个任务的目标文件是否已完好
    pub fn is_task_valid(&self, task: &DownloadTask) -> Result<bool, CoreError> {
        Ok(verify_file(&self.root.join(&task.relative_path), &task.sha1, task.size)?
            == VerifyOutcome::Valid)
    }
}

/// 下载汇总
#[derive(Debug, Clone)]
pub struct DownloadSummary {
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<(String, CoreError)>,
}

impl DownloadSummary {
    pub fn is_full_success(&self) -> bool {
        self.failed == 0
    }
}

/// 把相对路径解析为绝对路径（供 platform/app 层使用）
pub fn resolve_path(root: &Path, relative: &str) -> PathBuf {
    root.join(relative)
}
