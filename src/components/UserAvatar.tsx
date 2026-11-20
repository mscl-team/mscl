/** 用户头像：支持 data URL / 外置皮肤库头像 / 首字母兜底 */
import { useEffect, useState } from "react";

interface UserAvatarProps {
  src?: string | null;
  name: string;
  size?: number;
  className?: string;
  onClick?: () => void;
  title?: string;
}

export function UserAvatar({
  src,
  name,
  size = 40,
  className = "",
  onClick,
  title,
}: UserAvatarProps) {
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [src]);

  const inner = src && !broken ? (
    <img src={src} alt={name} width={size} height={size} draggable={false} />
  ) : (
    <span className="user-avatar-letter">{(name.trim()[0] || "P").toUpperCase()}</span>
  );

  const classes = `user-avatar ${className}`.trim();
  const style = { width: size, height: size };

  if (onClick) {
    return (
      <button
        type="button"
        className={`${classes} user-avatar-clickable`}
        style={style}
        onClick={onClick}
        title={title ?? name}
      >
        {inner}
      </button>
    );
  }

  return (
    <span className={classes} style={style} title={title ?? name}>
      {inner}
    </span>
  );
}
