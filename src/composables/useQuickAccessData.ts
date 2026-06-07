import { ref } from "vue";
import { seekMindApi } from "../services/seekMindApi";
import type { FavoriteView, IndexDirView, RecentDocumentView, SearchHistoryView } from "../types/SeekMind";

const quickDirs = ref<IndexDirView[]>([]);
const searchHistory = ref<SearchHistoryView[]>([]);
const recentDocuments = ref<RecentDocumentView[]>([]);
const favorites = ref<FavoriteView[]>([]);
const loadingQuickAccess = ref(false);

export const useQuickAccessData = () => {
  const loadQuickAccessData = async () => {
    loadingQuickAccess.value = true;
    try {
      const [dirs, history, recent, favoriteList] = await Promise.all([
        seekMindApi.listIndexDirs(),
        seekMindApi.listSearchHistory(10),
        seekMindApi.listRecentDocuments(8),
        seekMindApi.listFavorites(12),
      ]);

      quickDirs.value = dirs.filter((dir) => dir.enabled);
      searchHistory.value = history;
      recentDocuments.value = recent;
      favorites.value = favoriteList;
    } finally {
      loadingQuickAccess.value = false;
    }
  };

  return {
    quickDirs,
    searchHistory,
    recentDocuments,
    favorites,
    loadingQuickAccess,
    loadQuickAccessData,
  };
};
