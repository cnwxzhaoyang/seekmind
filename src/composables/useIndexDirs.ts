/**
 * @author MorningSun
 * @CreatedDate 2026/06/15
 * @Description 共享索引目录数据源，统一缓存当前应用内的目录列表并提供刷新入口。
 */
import { ref } from "vue";
import type { IndexDirView } from "../types/SeekMind";
import { seekMindApi } from "../services/seekMindApi";

const dirs = ref<IndexDirView[]>([]);
const dirsLoaded = ref(false);
const dirsRefreshing = ref(false);
let dirsRefreshPromise: Promise<IndexDirView[]> | null = null;

export const useIndexDirs = () => {
  const refreshIndexDirs = async (reason = "manual") => {
    if (dirsRefreshPromise) {
      return dirsRefreshPromise;
    }

    console.info("[SeekMind] refreshing shared index dirs", { reason });
    dirsRefreshing.value = true;

    dirsRefreshPromise = (async () => {
      const nextDirs = await seekMindApi.listIndexDirs();
      dirs.value = nextDirs;
      dirsLoaded.value = true;
      console.info("[SeekMind] shared index dirs refreshed", {
        reason,
        count: nextDirs.length,
      });
      return nextDirs;
    })()
      .catch((error) => {
        console.error("[SeekMind] refresh shared index dirs failed", {
          reason,
          error,
        });
        throw error;
      })
      .finally(() => {
        dirsRefreshPromise = null;
        dirsRefreshing.value = false;
      });

    return dirsRefreshPromise;
  };

  return {
    dirs,
    dirsLoaded,
    dirsRefreshing,
    refreshIndexDirs,
  };
};
