/**
 * @author MorningSun
 * @CreatedDate 2026/06/07
 * @Description QA 配置变更事件工具，负责在设置页与问答页之间同步模型连接更新。
 */
export const qaConfigUpdatedEventName = "seekmind:qa-config-updated";

export interface QaConfigUpdatedDetail {
  reason: string;
  updatedAt: number;
}

export const emitQaConfigUpdated = (reason: string) => {
  window.dispatchEvent(
    new CustomEvent<QaConfigUpdatedDetail>(qaConfigUpdatedEventName, {
      detail: {
        reason,
        updatedAt: Date.now(),
      },
    }),
  );
};

export const listenQaConfigUpdated = (handler: (detail: QaConfigUpdatedDetail) => void) => {
  const listener: EventListener = (event) => {
    const customEvent = event as CustomEvent<QaConfigUpdatedDetail>;
    handler(customEvent.detail);
  };

  window.addEventListener(qaConfigUpdatedEventName, listener);
  return () => window.removeEventListener(qaConfigUpdatedEventName, listener);
};
