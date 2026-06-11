/**
 * @author MorningSun
 * @CreatedDate 2026/06/11
 * @Description 日期格式化工具，统一将时间展示收敛到仅显示日期。
 */

export const formatSeekMindDateOnly = (value: string, locale = "zh-CN") => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
};
