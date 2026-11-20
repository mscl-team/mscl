/**
 * 更新日志弹窗（完全自定义 WebView 风格，独立于系统弹窗）
 *
 * 用于展示 GitHub release 的 markdown 更新日志，提供「立即更新 / 暂不更新」。
 * 轻量 markdown 渲染：标题、粗体、行内代码、代码块、列表、链接、分隔线。
 */

import { useEffect, type ReactNode } from "react";

export interface ReleaseUpdate {
  version: string;
  body: string;
  publishedAt: string;
  htmlUrl: string;
  kind: "plugin" | "launcher";
}

interface UpdateDialogProps {
  update: ReleaseUpdate | null;
  updating: boolean;
  progress?: string;
  onUpdate: () => void;
  onLater: () => void;
}

/** 极简 markdown 渲染：只处理安全的内联与块级结构，不注入 HTML */
function renderMarkdown(md: string): ReactNode[] {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const out: ReactNode[] = [];
  let inCode = false;
  let codeBuf: string[] = [];

  const inline = (text: string): ReactNode[] => {
    const nodes: ReactNode[] = [];
    const pattern = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
    let last = 0;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(text)) !== null) {
      if (match.index > last) nodes.push(text.slice(last, match.index));
      const token = match[0];
      if (token.startsWith("**")) {
        nodes.push(<strong key={match.index}>{token.slice(2, -2)}</strong>);
      } else if (token.startsWith("`")) {
        nodes.push(<code key={match.index}>{token.slice(1, -1)}</code>);
      } else {
        const linkMatch = /\[([^\]]+)\]\(([^)]+)\)/.exec(token);
        if (linkMatch) {
          nodes.push(
            <a key={match.index} href={linkMatch[2]} target="_blank" rel="noreferrer">
              {linkMatch[1]}
            </a>
          );
        }
      }
      last = match.index + token.length;
    }
    if (last < text.length) nodes.push(text.slice(last));
    return nodes;
  };

  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (line.trim().startsWith("```")) {
      if (inCode) {
        out.push(<pre key={`code-${index}`}><code>{codeBuf.join("\n")}</code></pre>);
        codeBuf = [];
        inCode = false;
      } else {
        inCode = true;
      }
      index += 1;
      continue;
    }
    if (inCode) {
      codeBuf.push(line);
      index += 1;
      continue;
    }
    const bullet = line.match(/^\s*[-*+]\s/);
    const ordered = line.match(/^\s*\d+\.\s/);
    if (bullet || ordered) {
      const items: ReactNode[] = [];
      while (index < lines.length) {
        const current = lines[index];
        const isBullet = current.match(/^\s*[-*+]\s/);
        const isOrdered = current.match(/^\s*\d+\.\s/);
        if (Boolean(isBullet) !== Boolean(bullet) || Boolean(isOrdered) !== Boolean(ordered)) break;
        const text = isBullet
          ? current.replace(/^\s*[-*+]\s/, "")
          : current.replace(/^\s*\d+\.\s/, "");
        items.push(<li key={`li-${index}`}>{inline(text)}</li>);
        index += 1;
      }
      out.push(
        ordered ? (
          <ol key={`list-${index}`}>{items}</ol>
        ) : (
          <ul key={`list-${index}`}>{items}</ul>
        )
      );
      continue;
    }
    if (/^#{1,6}\s/.test(line)) {
      const level = line.match(/^#+/)![0].length;
      const text = line.replace(/^#+\s/, "");
      const Tag = `h${Math.min(level, 4)}` as keyof React.JSX.IntrinsicElements;
      out.push(<Tag key={index}>{inline(text)}</Tag>);
    } else if (/^---+$/.test(line.trim())) {
      out.push(<hr key={index} />);
    } else if (line.trim()) {
      out.push(<p key={index}>{inline(line)}</p>);
    }
    index += 1;
  }
  if (inCode && codeBuf.length) {
    out.push(<pre key="code-tail"><code>{codeBuf.join("\n")}</code></pre>);
  }
  return out;
}

export function UpdateDialog({ update, updating, progress, onUpdate, onLater }: UpdateDialogProps) {
  useEffect(() => {
    if (!update) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !updating) onLater();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [update, updating, onLater]);

  if (!update) return null;

  return (
    <div className="update-dialog-backdrop" role="presentation">
      <div className="update-dialog" role="dialog" aria-modal="true" aria-label="更新日志">
        <div className="update-dialog-head">
          <div>
            <span className="update-dialog-badge">
              {update.kind === "plugin" ? "插件库更新" : "程序更新"}
            </span>
            <h3>发现新版本 {update.version}</h3>
          </div>
          {update.publishedAt && (
            <span className="update-dialog-date">{update.publishedAt.slice(0, 10)}</span>
          )}
        </div>

        <div className="update-dialog-body">
          {update.body.trim() ? (
            renderMarkdown(update.body)
          ) : (
            <p className="update-dialog-empty">该版本未提供更新日志。</p>
          )}
        </div>

        {progress && <div className="update-dialog-progress">{progress}</div>}

        <div className="update-dialog-actions">
          <button className="btn btn-ghost" onClick={onLater} disabled={updating}>
            暂不更新
          </button>
          <button className="btn btn-primary" onClick={onUpdate} disabled={updating}>
            {updating ? "正在更新…" : "立即更新"}
          </button>
        </div>
        <div className="update-dialog-note">
          立即更新会安全关闭启动器后执行；若更新中断，将自动回滚到当前版本。
        </div>
      </div>
    </div>
  );
}
