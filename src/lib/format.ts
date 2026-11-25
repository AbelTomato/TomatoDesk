const dateTimeFormat = new Intl.DateTimeFormat("zh-CN", {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

const relativeFormat = new Intl.RelativeTimeFormat("zh-CN", { numeric: "auto" });

export function formatDate(value: string | null): string {
  if (!value) return "尚未同步";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return dateTimeFormat.format(date);
}

/** 列表行用的相对时间：刚刚 / N 分钟前 / N 小时前 / N 天前，超过 30 天回落到日期。 */
export function formatRelative(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const seconds = Math.round((date.getTime() - Date.now()) / 1000);
  const absolute = Math.abs(seconds);
  if (absolute < 60) return "刚刚";
  if (absolute < 3600) return relativeFormat.format(Math.round(seconds / 60), "minute");
  if (absolute < 86_400) return relativeFormat.format(Math.round(seconds / 3600), "hour");
  if (absolute < 2_592_000) return relativeFormat.format(Math.round(seconds / 86_400), "day");
  return dateTimeFormat.format(date);
}

export function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
  return String(value);
}
