/** 启动器风格确认 / 输入提示窗，替代系统原生 confirm / prompt */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Icon } from "./Icon";
import { useT } from "../lib/i18n";

type DialogKind = "confirm" | "prompt" | "alert";

interface DialogOptions {
  title?: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
  defaultValue?: string;
  placeholder?: string;
  /** 输入该值才能确认，用于删除目录等二次确认 */
  expected?: string;
}

interface DialogState {
  kind: DialogKind;
  message: string;
  options: DialogOptions;
  value: string;
  error: string;
  resolve: (value: boolean | string | null) => void;
}

export interface ConfirmApi {
  confirm: (message: string, options?: DialogOptions) => Promise<boolean>;
  prompt: (message: string, options?: DialogOptions) => Promise<string | null>;
  alert: (message: string, options?: DialogOptions) => Promise<void>;
}

const ConfirmContext = createContext<ConfirmApi | null>(null);

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const [state, setState] = useState<DialogState | null>(null);

  const ask = useCallback(
    (kind: DialogKind, message: string, options: DialogOptions = {}) =>
      new Promise<boolean | string | null>((resolve) => {
        setState({
          kind,
          message,
          options,
          value: options.defaultValue ?? "",
          error: "",
          resolve,
        });
      }),
    []
  );

  const api = useMemo<ConfirmApi>(
    () => ({
      confirm: (message, options) =>
        ask("confirm", message, options).then((value) => value === true),
      prompt: (message, options) =>
        ask("prompt", message, options).then((value) =>
          typeof value === "string" ? value : null
        ),
      alert: (message, options) => ask("alert", message, options).then(() => undefined),
    }),
    [ask]
  );

  const finish = (result: boolean | string | null) => {
    if (!state) return;
    state.resolve(result);
    setState(null);
  };

  const accept = () => {
    if (!state) return;
    if (
      state.kind === "prompt" &&
      state.options.expected &&
      state.value !== state.options.expected
    ) {
      setState({
        ...state,
        error: t("dialog.mismatch", { word: state.options.expected }),
      });
      return;
    }
    finish(state.kind === "prompt" ? state.value : true);
  };

  const cancel = () => finish(state?.kind === "prompt" ? null : false);

  useEffect(() => {
    if (!state) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        cancel();
      } else if (event.key === "Enter" && state.kind !== "prompt") {
        event.preventDefault();
        accept();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!state) return <ConfirmContext.Provider value={api}>{children}</ConfirmContext.Provider>;

  const title =
    state.options.title ??
    (state.options.danger ? t("dialog.warningTitle") : t("dialog.confirmTitle"));
  const confirmLabel =
    state.options.confirmText ??
    (state.kind === "alert" ? t("common.close") : t("common.confirm"));
  const expected = state.options.expected;

  return (
    <ConfirmContext.Provider value={api}>
      {children}
      <div className="confirm-backdrop" role="presentation">
        <div className="confirm-dialog" role="alertdialog" aria-modal="true" aria-label={title}>
          <div className="confirm-head">
            <span className={`confirm-head-icon ${state.options.danger ? "is-danger" : ""}`}>
              <Icon name={state.options.danger ? "trash" : "info"} size={17} />
            </span>
            <h3>{title}</h3>
            <button className="btn-icon confirm-close" onClick={cancel} title={t("common.close")}>
              <Icon name="close" size={15} />
            </button>
          </div>

          <div className="confirm-body">
            <p>{state.message}</p>
            {state.kind === "prompt" && (
              <input
                className="settings-input"
                value={state.value}
                onChange={(e) =>
                  setState({ ...state, value: e.target.value, error: "" })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    accept();
                  }
                }}
                placeholder={state.options.placeholder ?? expected ?? ""}
                autoFocus
              />
            )}
            {(state.error ||
              (expected && state.value !== expected && state.kind === "prompt")) && (
              <div className="confirm-error">
                {state.error || t("dialog.mismatch", { word: expected ?? "" })}
              </div>
            )}
          </div>

          <div className="confirm-actions">
            {state.kind !== "alert" && (
              <button className="btn btn-ghost" onClick={cancel}>
                {state.options.cancelText ?? t("common.cancel")}
              </button>
            )}
            <button
              className={`btn ${state.options.danger ? "btn-danger" : "btn-primary"}`}
              onClick={accept}
              disabled={Boolean(expected && state.value !== expected)}
            >
              {confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </ConfirmContext.Provider>
  );
}

export function useConfirm(): ConfirmApi {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error("useConfirm must be used inside ConfirmProvider");
  return ctx;
}
