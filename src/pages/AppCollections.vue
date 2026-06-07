<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 知识库页面，负责集合管理、最近访问和条目详情编辑。
 */
defineOptions({
  name: "AppCollectionsPage",
});

import { computed, onActivated, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { BookMarked, ClipboardCopy, Eye, FileDown, FileText, Files, FolderPlus, Layers3, MessageSquareText, Pencil, Plus, RefreshCw, Search, SquareArrowOutUpRight, Trash2, X } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindContextMenu from "../components/docmind/DocMindContextMenu.vue";
import type { ContextMenuItem } from "../components/docmind/DocMindContextMenu.vue";
import DocMindFileIcon from "../components/docmind/DocMindFileIcon.vue";
import SplitPane from "../components/SplitPane.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { useInfoMessage } from "../composables/useInfoMessage";
import type { CollectionItemView, CollectionView, RecentViewEntry, TagView } from "../types/docmind";

const { t } = useI18n();
const router = useRouter();

const collections = ref<CollectionView[]>([]);
const recentViews = ref<RecentViewEntry[]>([]);
const collectionTags = ref<TagView[]>([]);
const itemTags = ref<TagView[]>([]);
const collectionItemTagsMap = ref<Record<string, TagView[]>>({});
const collectionItems = ref<CollectionItemView[]>([]);
const selectedCollectionId = ref("");
const selectedItemId = ref("");
// 右侧详情面板关闭时直接从 SplitPane 移除，避免只清空内容但仍然占位。
const showDetailPanel = ref(false);
const collectionFilter = ref("");
const itemTagFilter = ref("");
const collectionName = ref("");
const collectionDescription = ref("");
const collectionTagName = ref("");
const collectionSaving = ref(false);
const collectionsLoading = ref(false);
const itemsLoading = ref(false);
const errorMessage = ref("");
const { infoMessage } = useInfoMessage();
const collectionMenuVisible = ref(false);
const collectionMenuPosition = ref({ x: 0, y: 0 });
const collectionMenuTarget = ref<CollectionView | null>(null);
const itemMenuVisible = ref(false);
const itemMenuPosition = ref({ x: 0, y: 0 });
const itemMenuTarget = ref<CollectionItemView | null>(null);
const itemNoteDraft = ref("");
const itemTagName = ref("");
const itemSaving = ref(false);

const recentViewTypeLabel = (item: RecentViewEntry) => {
  switch (item.target_type) {
    case "collection":
      return t("page.collections.recentType.collection");
    case "qa_session":
      return t("page.collections.recentType.qaSession");
    case "document":
      return t("page.collections.recentType.document");
    default:
      return t("page.collections.recentType.chunk");
  }
};

const recentViewTypeIcon = (item: RecentViewEntry) => {
  switch (item.target_type) {
    case "collection":
      return BookMarked;
    case "qa_session":
      return MessageSquareText;
    case "document":
      return FileText;
    default:
      return Files;
  }
};

const selectedCollection = computed(
  () => collections.value.find((item) => item.id === selectedCollectionId.value) ?? null,
);

const selectedItem = computed(
  () => collectionItems.value.find((item) => item.id === selectedItemId.value) ?? null,
);

const splitPanels = computed(() => [
  { key: "left", initialSize: 320, minSize: 280 },
  { key: "middle", flex: true, minSize: 360 },
  ...(showDetailPanel.value ? [{ key: "right", initialSize: 360, minSize: 320 }] : []),
]);

const filteredCollections = computed(() => {
  const query = collectionFilter.value.trim().toLowerCase();
  if (!query) {
    return collections.value;
  }

  return collections.value.filter((collection) => {
    const haystack = [collection.name, collection.description, String(collection.item_count)].join(" ").toLowerCase();
    return haystack.includes(query);
  });
});

const itemTypeLabel = (item: CollectionItemView) => {
  switch (item.item_type) {
    case "document":
      return t("page.collections.itemType.document");
    case "search":
      return t("page.collections.itemType.search");
    case "qa_source":
      return t("page.collections.itemType.qaSource");
    default:
      return t("page.collections.itemType.chunk");
  }
};

const itemTypeTone = (item: CollectionItemView) => {
  switch (item.item_type) {
    case "document":
      return "success";
    case "search":
      return "warning";
    default:
      return "default";
  }
};

const availableItemTagFilters = computed(() => {
  const tagsById = new Map<string, TagView & { count: number }>();
  Object.values(collectionItemTagsMap.value).forEach((tags) => {
    tags.forEach((tag) => {
      const current = tagsById.get(tag.id);
      if (current) {
        current.count += 1;
        return;
      }
      tagsById.set(tag.id, { ...tag, count: 1 });
    });
  });

  return Array.from(tagsById.values()).sort((left, right) => {
    if (right.count !== left.count) {
      return right.count - left.count;
    }
    return left.name.localeCompare(right.name);
  });
});

const filteredCollectionItems = computed(() => {
  if (!itemTagFilter.value) {
    return collectionItems.value;
  }

  return collectionItems.value.filter((item) =>
    (collectionItemTagsMap.value[item.id] ?? []).some((tag) => tag.id === itemTagFilter.value),
  );
});

const refreshItemSelectionByFilter = () => {
  const visibleItems = filteredCollectionItems.value;
  if (visibleItems.length === 0) {
    selectedItemId.value = "";
    itemNoteDraft.value = "";
    showDetailPanel.value = false;
    void loadItemTags("");
    return;
  }

  if (!visibleItems.some((item) => item.id === selectedItemId.value)) {
    selectItem(visibleItems[0]);
  }
};

const toggleItemTagFilter = (tagId: string) => {
  itemTagFilter.value = itemTagFilter.value === tagId ? "" : tagId;
  refreshItemSelectionByFilter();
};

const clearItemTagFilter = () => {
  if (!itemTagFilter.value) {
    return;
  }
  itemTagFilter.value = "";
  if (!selectedItemId.value && collectionItems.value.length > 0) {
    selectItem(collectionItems.value[0]);
  }
};

const itemTypeIcon = (item: CollectionItemView) => {
  switch (item.item_type) {
    case "document":
      return BookMarked;
    case "search":
      return Search;
    case "qa_source":
      return Layers3;
    default:
      return Files;
  }
};

const resetEditor = () => {
  selectedCollectionId.value = "";
  collectionName.value = "";
  collectionDescription.value = "";
};

const syncEditorFromCollection = (collection: CollectionView | null) => {
  collectionName.value = collection?.name ?? "";
  collectionDescription.value = collection?.description ?? "";
};

const loadCollectionTags = async (collectionId: string) => {
  if (!collectionId) {
    collectionTags.value = [];
    collectionTagName.value = "";
    return;
  }

  try {
    collectionTags.value = await docmindApi.listTargetTags("collection", collectionId);
  } catch (error) {
    console.error("[DocMind] listTargetTags(collection) failed", error);
  }
};

const loadItemTags = async (itemId: string) => {
  if (!itemId) {
    itemTags.value = [];
    itemTagName.value = "";
    return;
  }

  try {
    itemTags.value = await docmindApi.listTargetTags("collection_item", itemId);
  } catch (error) {
    console.error("[DocMind] listTargetTags(collection_item) failed", error);
  }
};

const loadCollectionItemTags = async (items: CollectionItemView[]) => {
  if (items.length === 0) {
    collectionItemTagsMap.value = {};
    itemTagFilter.value = "";
    return;
  }

  const pairs = await Promise.all(items.map(async (item) => {
    try {
      const tags = await docmindApi.listTargetTags("collection_item", item.id);
      return [item.id, tags] as const;
    } catch (error) {
      console.error(`[DocMind] listTargetTags(collection_item) failed item=${item.id}`, error);
      return [item.id, [] as TagView[]] as const;
    }
  }));

  collectionItemTagsMap.value = Object.fromEntries(pairs);
  if (itemTagFilter.value && !availableItemTagFilters.value.some((tag) => tag.id === itemTagFilter.value)) {
    itemTagFilter.value = "";
  }
};

const loadCollections = async (preferSelected = true) => {
  collectionsLoading.value = true;
  errorMessage.value = "";
  try {
    const list = await docmindApi.listCollections();
    collections.value = list;
    if (selectedCollectionId.value && !list.some((item) => item.id === selectedCollectionId.value)) {
      selectedCollectionId.value = "";
      collectionItems.value = [];
      selectedItemId.value = "";
      resetEditor();
    }

    if (!selectedCollectionId.value) {
      const first = list[0] ?? null;
      if (first) {
        selectedCollectionId.value = first.id;
        syncEditorFromCollection(first);
        await loadItems(first.id, preferSelected);
        await loadCollectionTags(first.id);
        await docmindApi.recordRecentView("collection", first.id, first.name, first.description || "");
      } else {
        resetEditor();
        collectionItems.value = [];
        selectedItemId.value = "";
        collectionTags.value = [];
      }
    } else if (selectedCollectionId.value) {
      await loadItems(selectedCollectionId.value, true);
      await loadCollectionTags(selectedCollectionId.value);
    }
    await loadRecentViews();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.loadCollections"));
  } finally {
    collectionsLoading.value = false;
  }
};

const loadRecentViews = async () => {
  try {
    recentViews.value = await docmindApi.listRecentViews(8);
  } catch (error) {
    console.error("[DocMind] listRecentViews failed", error);
  }
};

const loadItems = async (collectionId: string, preserveSelected = true) => {
  if (!collectionId) {
    collectionItems.value = [];
    selectedItemId.value = "";
    itemNoteDraft.value = "";
    showDetailPanel.value = false;
    return;
  }

  itemsLoading.value = true;
  try {
    const items = await docmindApi.listCollectionItems(collectionId);
    collectionItems.value = items;
    await loadCollectionItemTags(items);

    const visibleItems = itemTagFilter.value
      ? items.filter((item) => (collectionItemTagsMap.value[item.id] ?? []).some((tag) => tag.id === itemTagFilter.value))
      : items;

    if (preserveSelected && selectedItemId.value && visibleItems.some((item) => item.id === selectedItemId.value)) {
      const current = items.find((item) => item.id === selectedItemId.value) ?? null;
      itemNoteDraft.value = current?.note ?? "";
      showDetailPanel.value = Boolean(current);
      if (current) {
        await loadItemTags(current.id);
      }
      return;
    }

    const next = visibleItems[0] ?? null;
    selectedItemId.value = next?.id ?? "";
    itemNoteDraft.value = next?.note ?? "";
    showDetailPanel.value = Boolean(next);
    await loadItemTags(next?.id ?? "");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.loadItems"));
  } finally {
    itemsLoading.value = false;
  }
};

const selectCollection = async (collection: CollectionView) => {
  selectedCollectionId.value = collection.id;
  syncEditorFromCollection(collection);
  await loadItems(collection.id, false);
  await loadCollectionTags(collection.id);
  await docmindApi.recordRecentView("collection", collection.id, collection.name, collection.description || "");
  await loadRecentViews();
};

const startNewCollection = () => {
  selectedCollectionId.value = "";
  resetEditor();
  collectionItems.value = [];
  collectionItemTagsMap.value = {};
  itemTagFilter.value = "";
  selectedItemId.value = "";
  itemNoteDraft.value = "";
  collectionTags.value = [];
  itemTags.value = [];
  collectionTagName.value = "";
  itemTagName.value = "";
  showDetailPanel.value = false;
};

const saveCollection = async () => {
  const name = collectionName.value.trim();
  if (!name || collectionSaving.value) {
    return;
  }

  collectionSaving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    if (selectedCollectionId.value) {
      const updated = await docmindApi.updateCollection(selectedCollectionId.value, {
        name,
        description: collectionDescription.value.trim(),
      });
      collections.value = collections.value.map((item) => (item.id === updated.id ? updated : item));
      syncEditorFromCollection(updated);
      infoMessage.value = t("page.collections.updated", { name: updated.name });
    } else {
      const created = await docmindApi.createCollection(name, collectionDescription.value.trim());
      collections.value = [created, ...collections.value.filter((item) => item.id !== created.id)];
      await selectCollection(created);
      infoMessage.value = t("page.collections.created", { name: created.name });
    }
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.saveCollection"));
  } finally {
    collectionSaving.value = false;
  }
};

const deleteCollection = async (collection: CollectionView) => {
  if (!window.confirm(t("page.collections.confirmDelete", { name: collection.name }))) {
    return;
  }

  try {
    await docmindApi.deleteCollection(collection.id);
    collections.value = collections.value.filter((item) => item.id !== collection.id);
    if (selectedCollectionId.value === collection.id) {
      const next = collections.value[0] ?? null;
      if (next) {
        await selectCollection(next);
      } else {
        startNewCollection();
      }
    }
    if (selectedCollectionId.value !== collection.id) {
      await loadRecentViews();
    }
    infoMessage.value = t("page.collections.deleted", { name: collection.name });
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.deleteCollection"));
  }
};

const selectItem = (item: CollectionItemView) => {
  selectedItemId.value = item.id;
  itemNoteDraft.value = item.note;
  showDetailPanel.value = true;
  void loadItemTags(item.id);
};

const closeSelectedItem = () => {
  console.debug("[DocMind] collection detail panel closed", {
    itemId: selectedItemId.value,
  });
  selectedItemId.value = "";
  itemNoteDraft.value = "";
  itemTagName.value = "";
  showDetailPanel.value = false;
  void loadItemTags("");
};

const saveItemNote = async () => {
  if (!selectedItem.value || itemSaving.value) {
    return;
  }

  itemSaving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";
  try {
    const updated = await docmindApi.updateCollectionItemNote(selectedItem.value.id, {
      note: itemNoteDraft.value,
    });
    collectionItems.value = collectionItems.value.map((item) => (item.id === updated.id ? updated : item));
    selectedItemId.value = updated.id;
    itemNoteDraft.value = updated.note;
    infoMessage.value = t("page.collections.noteSaved");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.saveItem"));
  } finally {
    itemSaving.value = false;
  }
};

const addCollectionTag = async () => {
  if (!selectedCollectionId.value || !collectionTagName.value.trim()) {
    return;
  }

  try {
    await docmindApi.addTagToTarget("collection", selectedCollectionId.value, collectionTagName.value);
    collectionTagName.value = "";
    await loadCollectionTags(selectedCollectionId.value);
    infoMessage.value = t("page.collections.tagAdded");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.tagSave"));
  }
};

const addItemTag = async () => {
  if (!selectedItem.value || !itemTagName.value.trim()) {
    return;
  }

  try {
    await docmindApi.addTagToTarget("collection_item", selectedItem.value.id, itemTagName.value);
    itemTagName.value = "";
    await loadItemTags(selectedItem.value.id);
    infoMessage.value = t("page.collections.tagAdded");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.tagSave"));
  }
};

const removeCollectionTag = async (tag: TagView) => {
  if (!selectedCollectionId.value) {
    return;
  }

  try {
    await docmindApi.removeTagFromTarget("collection", selectedCollectionId.value, tag.id);
    await loadCollectionTags(selectedCollectionId.value);
    infoMessage.value = t("page.collections.tagRemoved");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.tagRemove"));
  }
};

const removeItemTag = async (tag: TagView) => {
  if (!selectedItem.value) {
    return;
  }

  try {
    await docmindApi.removeTagFromTarget("collection_item", selectedItem.value.id, tag.id);
    await loadItemTags(selectedItem.value.id);
    infoMessage.value = t("page.collections.tagRemoved");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.tagRemove"));
  }
};

const removeItem = async (item: CollectionItemView) => {
  if (!window.confirm(t("page.collections.confirmRemoveItem", { name: item.title }))) {
    return;
  }

  try {
    await docmindApi.removeCollectionItem(item.id);
    collectionItems.value = collectionItems.value.filter((entry) => entry.id !== item.id);
    if (selectedItemId.value === item.id) {
      const next = collectionItems.value[0] ?? null;
      selectedItemId.value = next?.id ?? "";
      itemNoteDraft.value = next?.note ?? "";
      showDetailPanel.value = Boolean(next);
      await loadItemTags(next?.id ?? "");
    }
    infoMessage.value = t("page.collections.itemRemoved");
    if (selectedCollection.value) {
      const updated = await docmindApi.listCollections();
      collections.value = updated;
    }
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.removeItem"));
  }
};

const openItemFile = async (item: CollectionItemView) => {
  if (!item.path.trim()) return;
  await docmindApi.openFile(item.path);
};

const openRecentView = async (item: RecentViewEntry) => {
  switch (item.target_type) {
    case "collection": {
      const collection = collections.value.find((entry) => entry.id === item.target_id);
      if (collection) {
        await selectCollection(collection);
      } else {
        await loadCollections(false);
        const next = collections.value.find((entry) => entry.id === item.target_id);
        if (next) {
          await selectCollection(next);
        }
      }
      break;
    }
    case "qa_session":
      await router.push({ path: "/qa", query: { session: item.target_id } });
      break;
    default:
      if (item.path.trim()) {
        await docmindApi.openFile(item.path);
      }
      break;
  }
};

const quickLookItem = async (item: CollectionItemView) => {
  if (!item.path.trim()) return;
  await docmindApi.quickLookFile(item.path);
};

const copyItemPath = async (item: CollectionItemView) => {
  await navigator.clipboard.writeText(item.path);
  infoMessage.value = t("page.collections.copiedPath");
};

const copyItemCitation = async (item: CollectionItemView) => {
  const lines = [
    item.title,
    item.path ? `路径：${item.path}` : "",
    item.title_path ? `定位：${item.title_path}` : "",
    item.snippet ? `摘录：${item.snippet}` : "",
    item.note ? `备注：${item.note}` : "",
  ].filter(Boolean);
  await navigator.clipboard.writeText(lines.join("\n"));
  infoMessage.value = t("page.collections.copiedCitation");
};

const exportCollectionMarkdown = async (collection: CollectionView) => {
  const defaultName = `${collection.name}.md`.replace(/[\\/:*?"<>|]+/g, "-");
  const path = await save({
    title: t("page.collections.exportTitle"),
    defaultPath: defaultName,
    filters: [{ name: "Markdown", extensions: ["md"] }],
  });
  if (!path) {
    return;
  }

  try {
    const savedPath = await docmindApi.exportCollectionMarkdown(collection.id, path);
    infoMessage.value = t("page.collections.exportedMarkdown", { path: savedPath });
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.collections.error.exportFailed"));
  }
};

const collectionContextMenuItems = computed<ContextMenuItem[]>(() => {
  if (!collectionMenuTarget.value) {
    return [];
  }

  return [
    {
      key: "export",
      label: t("page.collections.exportMarkdown"),
      icon: FileDown,
      handler: () => {
        void exportCollectionMarkdown(collectionMenuTarget.value!);
      },
    },
    { key: "divider-1", label: "", divider: true },
    {
      key: "delete",
      label: t("page.collections.deleteCollection"),
      icon: Trash2,
      danger: true,
      handler: () => {
        void deleteCollection(collectionMenuTarget.value!);
      },
    },
  ];
});

const itemContextMenuItems = computed<ContextMenuItem[]>(() => {
  if (!itemMenuTarget.value) {
    return [];
  }

  return [
    {
      key: "openFile",
      label: t("page.collections.openFile"),
      icon: SquareArrowOutUpRight,
      handler: () => {
        void openItemFile(itemMenuTarget.value!);
      },
    },
    {
      key: "quickLook",
      label: t("page.collections.quickLook"),
      icon: Eye,
      handler: () => {
        void quickLookItem(itemMenuTarget.value!);
      },
    },
    { key: "divider-copy", label: "", divider: true },
    {
      key: "copyPath",
      label: t("page.collections.copyPath"),
      icon: ClipboardCopy,
      handler: () => {
        void copyItemPath(itemMenuTarget.value!);
      },
    },
    {
      key: "copyCitation",
      label: t("page.collections.copyCitation"),
      icon: Files,
      handler: () => {
        void copyItemCitation(itemMenuTarget.value!);
      },
    },
    { key: "divider-remove", label: "", divider: true },
    {
      key: "remove",
      label: t("page.collections.removeItem"),
      icon: Trash2,
      danger: true,
      handler: () => {
        void removeItem(itemMenuTarget.value!);
      },
    },
  ];
});

const openCollectionMenu = (collection: CollectionView, event: MouseEvent) => {
  collectionMenuTarget.value = collection;
  collectionMenuPosition.value = { x: event.clientX, y: event.clientY };
  collectionMenuVisible.value = true;
};

const openItemMenu = (item: CollectionItemView, event: MouseEvent) => {
  itemMenuTarget.value = item;
  itemMenuPosition.value = { x: event.clientX, y: event.clientY };
  itemMenuVisible.value = true;
};

onMounted(async () => {
  await loadCollections(false);
  await loadRecentViews();
  if (collections.value.length > 0 && !selectedCollectionId.value) {
    await selectCollection(collections.value[0]);
  }
});

onActivated(async () => {
  await loadCollections(false);
  await loadRecentViews();
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-panel text-primary">
    <header class="docmind-page-topbar">
      <div class="docmind-page-title-area">
        <div class="docmind-page-title-row">
          <span class="docmind-page-header-icon" aria-hidden="true">
            <BookMarked :size="17" />
          </span>
          <h1 class="docmind-page-title">{{ t("page.collections.title") }}</h1>
        </div>
        <p class="docmind-page-subtitle">{{ t("page.collections.subtitle") }}</p>
      </div>
      <div class="docmind-page-actions">
        <button
          class="inline-flex items-center gap-2 rounded-lg border border-default bg-surface px-3 py-2 text-sm text-secondary transition hover:border-accent hover:text-primary"
          type="button"
          :disabled="collectionsLoading"
          @click="loadCollections(false)"
        >
          <RefreshCw :size="16" />
          {{ t("page.collections.resync") }}
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-lg bg-accent px-3 py-2 text-sm font-medium text-white transition hover:bg-accent-strong"
          type="button"
          :disabled="!selectedCollection"
          @click="selectedCollection && void exportCollectionMarkdown(selectedCollection)"
        >
          <FileDown :size="16" />
          {{ t("page.collections.exportMarkdown") }}
        </button>
      </div>
    </header>

    <div v-if="errorMessage" class="border-b border-danger-soft bg-danger-soft px-5 py-3 text-sm text-danger">
      {{ errorMessage }}
    </div>
    <div v-if="infoMessage" class="border-b border-emerald-soft bg-emerald-soft px-5 py-3 text-sm text-success">
      {{ infoMessage }}
    </div>

    <SplitPane class="min-h-0 flex-1" :panels="splitPanels">
      <template #left>
        <aside class="flex min-h-0 flex-1 flex-col overflow-hidden border-r border-default bg-sidebar/30">
          <div class="px-4 py-3">
            <div class="flex items-center justify-between gap-2">
              <div>
                <div class="text-sm font-semibold text-primary">{{ t("page.collections.editorTitle") }}</div>
                <div class="mt-1 text-xs text-muted">{{ t("page.collections.editorDesc") }}</div>
              </div>
              <button
                class="inline-flex h-8 w-8 items-center justify-center rounded-md border border-default bg-surface text-muted transition hover:border-accent hover:text-primary"
                type="button"
                :title="t('page.collections.newCollection')"
                @click="startNewCollection"
              >
                <Plus :size="15" />
              </button>
            </div>
            <div class="mt-2 space-y-2">
              <input
                v-model="collectionName"
                class="w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary placeholder:text-muted focus:border-accent focus:outline-none"
                type="text"
                :placeholder="t('page.collections.namePlaceholder')"
              >
              <textarea
                v-model="collectionDescription"
                class="min-h-[64px] w-full resize-none rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary placeholder:text-muted focus:border-accent focus:outline-none"
                :placeholder="t('page.collections.descriptionPlaceholder')"
              />
            </div>
            <div class="mt-2 flex items-center gap-2">
              <button
                class="inline-flex items-center gap-2 rounded-md bg-accent px-3 py-2 text-sm font-medium text-white transition hover:bg-accent-strong disabled:cursor-not-allowed disabled:opacity-60"
                type="button"
                :disabled="collectionSaving || !collectionName.trim()"
                @click="saveCollection"
              >
                <FolderPlus :size="16" />
                {{ selectedCollectionId ? t("page.collections.saveChanges") : t("page.collections.create") }}
              </button>
              <button
                class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm text-secondary transition hover:border-accent hover:text-primary"
                type="button"
                @click="startNewCollection"
              >
                {{ t("page.collections.reset") }}
              </button>
            </div>
          </div>

          <div class="border-b border-default px-4 py-2.5">
            <div class="relative">
              <Search class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted" :size="15" />
              <input
                v-model="collectionFilter"
                class="w-full rounded-md border border-default bg-surface py-2 pl-9 pr-3 text-sm text-primary placeholder:text-muted focus:border-accent focus:outline-none"
                type="text"
                :placeholder="t('page.collections.filterPlaceholder')"
              >
            </div>
          </div>

          <div class="border-b border-default px-4 py-3">
            <div class="mb-2 flex items-center justify-between gap-2">
              <div class="text-sm font-semibold text-primary">{{ t("page.collections.recentTitle") }}</div>
              <div class="text-[11px] text-muted">{{ t("page.collections.recentSubtitle") }}</div>
            </div>
            <div v-if="recentViews.length === 0" class="text-[11px] text-muted">
              {{ t("page.collections.recentEmpty") }}
            </div>
            <div v-else class="max-h-48 space-y-2 overflow-y-auto pr-1">
              <button
                v-for="item in recentViews"
                :key="`${item.target_type}:${item.target_id}`"
                class="flex w-full items-start justify-between gap-3 rounded-md border border-default bg-surface px-3 py-2 text-left transition hover:bg-accent-soft"
                type="button"
                @click="openRecentView(item)"
              >
                <div class="flex min-w-0 items-start gap-2">
                  <span class="mt-0.5 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-panel text-accent">
                    <component :is="recentViewTypeIcon(item)" :size="14" />
                  </span>
                  <div class="min-w-0">
                    <div class="truncate text-sm font-medium text-primary">{{ item.title }}</div>
                    <div class="mt-1 truncate text-[11px] text-muted">{{ item.path || t("common.none") }}</div>
                  </div>
                </div>
                <div class="shrink-0 text-right">
                  <DocMindBadge tone="default">{{ recentViewTypeLabel(item) }}</DocMindBadge>
                  <div class="mt-1 text-[10px] text-dim">{{ item.viewed_at }}</div>
                </div>
              </button>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            <div v-if="collectionsLoading" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else-if="filteredCollections.length === 0" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
              {{ t("page.collections.emptyCollections") }}
            </div>
            <div v-else class="space-y-2">
              <div
                v-for="collection in filteredCollections"
                :key="collection.id"
                class="rounded-md border border-default bg-surface px-3 py-2.5 transition"
                :class="selectedCollectionId === collection.id ? 'bg-accent-soft shadow-card' : 'hover:bg-surface-hover'"
                @click="selectCollection(collection)"
                @contextmenu.prevent="openCollectionMenu(collection, $event)"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="truncate text-sm font-medium text-primary">{{ collection.name }}</div>
                    <div class="mt-0.5 max-h-9 overflow-hidden text-xs leading-4 text-muted">
                      {{ collection.description || t("page.collections.noDescription") }}
                    </div>
                  </div>
                  <DocMindBadge tone="default">{{ collection.item_count }}</DocMindBadge>
                </div>
                <div class="mt-1.5 text-[11px] text-dim">{{ collection.updated_at }}</div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <template #middle>
        <section class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
          <div class="docmind-section-header docmind-section-header--balanced">
            <span class="card-icon docmind-page-header-icon"><Layers3 :size="17" /></span>
            <div class="min-w-0">
              <div class="text-sm font-semibold text-primary">
                {{ selectedCollection?.name || t("page.collections.noCollectionSelected") }}
              </div>
              <div class="mt-1 text-xs text-muted">
                {{ selectedCollection?.description || t("page.collections.noDescription") }}
              </div>
            </div>
            <DocMindBadge tone="default">{{ t("page.collections.itemCount", { count: collectionItems.length }) }}</DocMindBadge>
          </div>
          <div v-if="selectedCollection" class="docmind-content-block docmind-content-block--tight-top">
            <div class="flex items-center justify-between gap-2">
              <div class="docmind-content-block-title">{{ t("page.collections.tags") }}</div>
              <div class="text-[10px] text-muted">{{ collectionTags.length }}</div>
            </div>
            <div class="mt-2 flex gap-2 overflow-x-auto pb-1">
              <div
                v-for="tag in collectionTags"
                :key="tag.id"
                class="inline-flex shrink-0 items-center gap-1 rounded-full border border-default bg-panel px-2 py-0.5 text-[11px] text-secondary"
              >
                <span class="max-w-[7rem] truncate">{{ tag.name }}</span>
                <button
                  class="inline-flex h-4 w-4 items-center justify-center rounded-full text-muted transition hover:bg-danger-soft hover:text-danger"
                  type="button"
                  :title="t('page.collections.removeTag')"
                  @click.stop="removeCollectionTag(tag)"
                >
                  <Trash2 :size="10" />
                </button>
              </div>
              <span v-if="collectionTags.length === 0" class="text-[11px] text-muted">{{ t("page.collections.noTags") }}</span>
            </div>
            <div class="mt-2 flex items-center gap-2">
              <input
                v-model="collectionTagName"
                class="min-w-0 flex-1 rounded-md border border-default bg-panel px-2.5 py-1.5 text-xs text-primary placeholder:text-muted focus:border-accent focus:outline-none"
                type="text"
                :placeholder="t('page.collections.tagPlaceholder')"
                @keydown.enter.prevent="addCollectionTag"
              >
              <button
                class="inline-flex items-center gap-1.5 rounded-md border border-default bg-surface px-2.5 py-1.5 text-xs text-secondary transition hover:border-accent hover:text-primary"
                type="button"
                :disabled="!collectionTagName.trim()"
                @click="addCollectionTag"
              >
                <Plus :size="13" />
                {{ t("page.collections.addTag") }}
              </button>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto p-4 pt-3">
            <div v-if="!selectedCollection" class="rounded-lg border border-dashed border-default bg-surface px-4 py-8 text-center text-xs text-muted">
              {{ t("page.collections.emptyCollections") }}
            </div>
            <div v-else-if="itemsLoading" class="rounded-lg border border-dashed border-default bg-surface px-4 py-8 text-center text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else class="space-y-3">
              <div v-if="availableItemTagFilters.length > 0" class="rounded-lg border border-default bg-surface px-3 py-3">
                <div class="flex items-center justify-between gap-2">
                  <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                    {{ t("page.collections.itemTagFilter") }}
                  </div>
                  <button
                    v-if="itemTagFilter"
                    class="text-[11px] text-muted transition hover:text-primary"
                    type="button"
                    @click="clearItemTagFilter"
                  >
                    {{ t("page.collections.clearFilter") }}
                  </button>
                </div>
                <div class="mt-2 flex flex-wrap gap-2">
                  <button
                    v-for="tag in availableItemTagFilters"
                    :key="tag.id"
                    class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition"
                    :class="itemTagFilter === tag.id ? 'border-accent bg-accent-soft text-primary' : 'border-default bg-panel text-secondary hover:bg-surface-hover hover:text-primary'"
                    type="button"
                    @click="toggleItemTagFilter(tag.id)"
                  >
                    <span class="max-w-[8rem] truncate">{{ tag.name }}</span>
                    <DocMindBadge tone="default">{{ tag.count }}</DocMindBadge>
                  </button>
                </div>
              </div>
              <div v-if="filteredCollectionItems.length === 0" class="rounded-lg border border-dashed border-default bg-surface px-4 py-8 text-center text-xs text-muted">
                {{ itemTagFilter ? t("page.collections.noFilteredItems") : t("page.collections.emptyItems") }}
              </div>
              <article
                v-else
                v-for="item in filteredCollectionItems"
                :key="item.id"
                class="cursor-pointer rounded-lg border border-default bg-surface p-3 transition"
                :class="selectedItemId === item.id ? 'bg-accent-soft shadow-card' : 'hover:bg-surface-hover'"
                @click="selectItem(item)"
                @contextmenu.prevent="openItemMenu(item, $event)"
              >
                <div class="flex items-start gap-3">
                  <DocMindFileIcon :ext="(item.path.split('.').pop() || item.item_type).slice(0, 8)" />
                  <div class="min-w-0 flex-1">
                    <div class="flex items-start justify-between gap-2">
                      <div class="min-w-0">
                        <div class="truncate text-sm font-medium text-primary">{{ item.title }}</div>
                        <div class="mt-1 text-[11px] text-muted">{{ item.path || t("common.none") }}</div>
                      </div>
                      <DocMindBadge :tone="itemTypeTone(item)">{{ itemTypeLabel(item) }}</DocMindBadge>
                    </div>
                    <div v-if="item.title_path" class="mt-2 text-[11px] text-dim">
                      {{ t("page.collections.location") }}：{{ item.title_path }}
                    </div>
                    <div class="mt-2 max-h-[4.5rem] overflow-hidden text-sm leading-6 text-secondary">
                      {{ item.snippet || t("page.collections.noSnippet") }}
                    </div>
                    <div class="mt-2 flex flex-wrap gap-2 text-[11px] text-dim">
                      <span>{{ item.created_at }}</span>
                      <span v-if="item.note">·</span>
                      <span v-if="item.note">{{ t("page.collections.hasNote") }}</span>
                    </div>
                  </div>
                </div>
              </article>
            </div>
          </div>
        </section>
      </template>

      <template v-if="showDetailPanel" #right>
        <aside class="flex min-h-0 flex-1 flex-col overflow-hidden border-l border-default bg-panel/80">
          <div class="docmind-section-header docmind-section-header--balanced">
            <span class="card-icon docmind-page-header-icon"><BookMarked :size="17" /></span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold text-primary">{{ t("page.collections.detailTitle") }}</div>
              <div class="mt-1 text-xs text-muted">{{ t("page.collections.detailDesc") }}</div>
            </div>
            <button
              v-if="selectedItem"
              class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-default bg-surface text-secondary hover:bg-surface-hover hover:text-primary"
              type="button"
              :title="t('common.close')"
              @click="closeSelectedItem"
            >
              <X :size="14" />
            </button>
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4 pt-1">
            <div v-if="!selectedItem" class="rounded-lg border border-dashed border-default bg-surface px-4 py-8 text-center text-xs text-muted">
              {{ t("page.collections.noItemSelected") }}
            </div>
            <div v-else class="space-y-2">
              <div class="docmind-content-block docmind-content-block--super-tight-top">
                <div class="docmind-section-header docmind-section-header--compact">
                  <span class="card-icon docmind-page-header-icon">
                    <component :is="itemTypeIcon(selectedItem)" :size="17" />
                  </span>
                  <div class="min-w-0">
                    <div class="text-base font-semibold text-primary">{{ selectedItem.title }}</div>
                    <div class="mt-1 break-all text-xs text-muted">{{ selectedItem.path || t("common.none") }}</div>
                  </div>
                  <DocMindBadge :tone="itemTypeTone(selectedItem)">{{ itemTypeLabel(selectedItem) }}</DocMindBadge>
                </div>
                <div v-if="selectedItem.title_path" class="mt-2 text-sm leading-6 text-primary">
                  <span class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                    {{ t("page.collections.location") }}
                  </span>
                  <div class="mt-1">{{ selectedItem.title_path }}</div>
                </div>
              </div>

              <div class="docmind-content-block">
                <div class="flex items-center justify-between gap-2">
                  <div class="docmind-content-block-title">{{ t("page.collections.tags") }}</div>
                  <div class="text-[10px] text-muted">{{ itemTags.length }}</div>
                </div>
                <div class="mt-2 flex gap-2 overflow-x-auto pb-1">
                  <div
                    v-for="tag in itemTags"
                    :key="tag.id"
                    class="inline-flex shrink-0 items-center gap-1 rounded-full border border-default bg-panel px-2 py-0.5 text-[11px] text-secondary"
                  >
                    <span class="max-w-[7rem] truncate">{{ tag.name }}</span>
                    <button
                      class="inline-flex h-4 w-4 items-center justify-center rounded-full text-muted transition hover:bg-danger-soft hover:text-danger"
                      type="button"
                      :title="t('page.collections.removeTag')"
                      @click.stop="removeItemTag(tag)"
                    >
                      <Trash2 :size="10" />
                    </button>
                  </div>
                  <span v-if="itemTags.length === 0" class="text-[11px] text-muted">{{ t("page.collections.noTags") }}</span>
                </div>
                <div class="mt-2 flex items-center gap-2">
                  <input
                    v-model="itemTagName"
                    class="min-w-0 flex-1 rounded-md border border-default bg-panel px-2.5 py-1.5 text-xs text-primary placeholder:text-muted focus:border-accent focus:outline-none"
                    type="text"
                    :placeholder="t('page.collections.tagPlaceholder')"
                    @keydown.enter.prevent="addItemTag"
                  >
                  <button
                    class="inline-flex items-center gap-1.5 rounded-md border border-default bg-surface px-2.5 py-1.5 text-xs text-secondary transition hover:border-accent hover:text-primary"
                    type="button"
                    :disabled="!itemTagName.trim()"
                    @click="addItemTag"
                  >
                    <Plus :size="13" />
                    {{ t("page.collections.addTag") }}
                  </button>
                </div>
              </div>

              <div class="docmind-content-block">
                <div class="docmind-content-block-title">{{ t("page.collections.snippet") }}</div>
                <div class="whitespace-pre-wrap text-sm leading-7 text-primary">
                  {{ selectedItem.snippet || t("page.collections.noSnippet") }}
                </div>
              </div>

              <div class="docmind-content-block">
                <div class="docmind-content-block-title">{{ t("page.collections.note") }}</div>
                <textarea
                  v-model="itemNoteDraft"
                  class="min-h-[160px] w-full resize-y rounded-lg border border-default bg-panel px-3 py-2 text-sm text-primary placeholder:text-muted focus:border-accent focus:outline-none"
                  :placeholder="t('page.collections.notePlaceholder')"
                />
                <div class="mt-3 flex items-center gap-2">
                  <button
                    class="inline-flex items-center gap-2 rounded-lg bg-accent px-3 py-2 text-sm font-medium text-white transition hover:bg-accent-strong disabled:cursor-not-allowed disabled:opacity-60"
                    type="button"
                    :disabled="itemSaving"
                    @click="saveItemNote"
                  >
                    <Pencil :size="16" />
                    {{ t("page.collections.saveNote") }}
                  </button>
                </div>
              </div>

              <div class="docmind-content-block">
                <div class="grid grid-cols-2 gap-x-4 gap-y-2 text-xs leading-5 text-muted">
                  <div>{{ t("page.collections.createdAt") }}：{{ selectedItem.created_at }}</div>
                  <div>{{ t("page.collections.updatedAt") }}：{{ selectedItem.updated_at }}</div>
                  <div class="break-all">{{ t("page.collections.collectionId") }}：{{ selectedItem.collection_id }}</div>
                  <div class="break-all">{{ t("page.collections.sourceMeta") }}：{{ selectedItem.source_meta_json || t("common.none") }}</div>
                </div>
              </div>
            </div>
          </div>
        </aside>
      </template>
    </SplitPane>

    <DocMindContextMenu
      v-if="collectionMenuVisible"
      :x="collectionMenuPosition.x"
      :y="collectionMenuPosition.y"
      :items="collectionContextMenuItems"
      @close="collectionMenuVisible = false"
    />
    <DocMindContextMenu
      v-if="itemMenuVisible"
      :x="itemMenuPosition.x"
      :y="itemMenuPosition.y"
      :items="itemContextMenuItems"
      @close="itemMenuVisible = false"
    />
  </div>
</template>
