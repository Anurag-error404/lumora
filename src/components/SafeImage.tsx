import {
  useEffect,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";

export function SafeImage({
  src,
  alt,
  className,
  loading,
  fallback,
  onClick,
}: {
  src: string | null | undefined;
  alt: string;
  className?: string;
  loading?: "eager" | "lazy";
  fallback: ReactNode;
  onClick?: (event: ReactMouseEvent<HTMLImageElement>) => void;
}) {
  const [failed, setFailed] = useState(false);

  useEffect(() => setFailed(false), [src]);

  if (!src || failed) return <>{fallback}</>;
  return (
    <img
      src={src}
      alt={alt}
      className={className}
      loading={loading}
      onError={() => setFailed(true)}
      onClick={onClick}
    />
  );
}
