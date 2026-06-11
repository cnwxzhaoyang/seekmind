/**
 * @author MorningSun
 * @CreatedDate 2026/06/10
 * @Description 快捷访问数据刷新事件工具，用于同步侧栏最近搜索、最近打开和收藏数据。
 */
export const quickAccessUpdatedEventName = "seekmind:quick-access-updated";

export interface QuickAccessUpdatedDetail {
  reason: string;
  updatedAt: number;
}

export const emitQuickAccessUpdated = (reason: string) => {
  window.dispatchEvent(
    new CustomEvent<QuickAccessUpdatedDetail>(quickAccessUpdatedEventName, {
      detail: {
        reason,
        updatedAt: Date.now(),
      },
    }),
  );
};

export const listenQuickAccessUpdated = (handler: (detail: QuickAccessUpdatedDetail) => void) => {
  const listener: EventListener = (event) => {
    const customEvent = event as CustomEvent<QuickAccessUpdatedDetail>;
    handler(customEvent.detail);
  };

  window.addEventListener(quickAccessUpdatedEventName, listener);
  return () => window.removeEventListener(quickAccessUpdatedEventName, listener);
};
