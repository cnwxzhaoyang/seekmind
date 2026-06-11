<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 知识库页面，负责集合管理和条目详情编辑。
 */
defineOptions({
  name: "AppCollectionsPage",
});

import { computed, onActivated, onMounted, ref } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { BookMarked, ClipboardCopy, Eye, FileDown, Files, FolderPlus, Layers3, Pencil, Plus, RefreshCw, Search, SquareArrowOutUpRight, Trash2, X } from "lucide-vue-next";
import SeekMindBadge from "../components/SeekMind/SeekMindBadge.vue";
import SeekMindDetailPanel from "../components/SeekMind/SeekMindDetailPanel.vue";
import SeekMindDetailSection from "../components/SeekMind/SeekMindDetailSection.vue";
import SeekMindContextMenu from "../components/SeekMind/SeekMindContextMenu.vue";
import SeekMindToast from "../components/SeekMind/SeekMindToast.vue";
import type { ContextMenuItem } from "../components/SeekMind/SeekMindContextMenu.vue";
import SeekMindFileIcon from "../components/SeekMind/SeekMindFileIcon.vue";
import SplitPane from "../components/SplitPane.vue";
import { seekMindApi, formatSeekMindError } from "../services/seekMindApi";
import { useInfoMessage } from "../composables/useInfoMessage";
import { formatSeekMindDateOnly } from "../utils/dateFormat";
import type { CollectionItemView, CollectionView, TagView } from "../types/SeekMind";

const { t, locale } = useI18n();
const collections = ref<CollectionView[]>([]);
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
const collectionTagInputRef = ref<HTMLInputElement | null>(null);
const collectionSaving = ref(false);
const collectionDialogVisible = ref(false);
const collectionDialogMode = ref<"create" | "edit">("create");
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
const itemTagInputRef = ref<HTMLInputElement | null>(null);
const itemSaving = ref(false);

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

const resetCollectionDialog = () => {
  collectionName.value = "";
  collectionDescription.value = "";
};

const focusCollectionTagInput = () => {
  collectionTagInputRef.value?.focus();
};

const focusItemTagInput = () => {
  itemTagInputRef.value?.focus();
};

const syncCollectionDialogFromCollection = (collection: CollectionView | null) => {
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
    collectionTags.value = await seekMindApi.listTargetTags("collection", collectionId);
  } catch (error) {
    console.error("[SeekMind] listTargetTags(collection) failed", error);
  }
};

const loadItemTags = async (itemId: string) => {
  if (!itemId) {
    itemTags.value = [];
    itemTagName.value = "";
    return;
  }

  try {
    itemTags.value = await seekMindApi.listTargetTags("collection_item", itemId);
  } catch (error) {
    console.error("[SeekMind] listTargetTags(collection_item) failed", error);
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
      const tags = await seekMindApi.listTargetTags("collection_item", item.id);
      return [item.id, tags] as const;
    } catch (error) {
      console.error(`[SeekMind] listTargetTags(collection_item) failed item=${item.id}`, error);
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
    const list = await seekMindApi.listCollections();
    collections.value = list;
    if (selectedCollectionId.value && !list.some((item) => item.id === selectedCollectionId.value)) {
      selectedCollectionId.value = "";
      collectionItems.value = [];
      selectedItemId.value = "";
      resetCollectionDialog();
    }

    if (!selectedCollectionId.value) {
      const first = list[0] ?? null;
      if (first) {
        selectedCollectionId.value = first.id;
        await loadItems(first.id, preferSelected);
        await loadCollectionTags(first.id);
        await seekMindApi.recordRecentView("collection", first.id, first.name, first.description || "");
      } else {
        resetCollectionDialog();
        collectionItems.value = [];
        selectedItemId.value = "";
        collectionTags.value = [];
      }
    } else if (selectedCollectionId.value) {
      await loadItems(selectedCollectionId.value, true);
      await loadCollectionTags(selectedCollectionId.value);
    }
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.loadCollections"));
  } finally {
    collectionsLoading.value = false;
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
    const items = await seekMindApi.listCollectionItems(collectionId);
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
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.loadItems"));
  } finally {
    itemsLoading.value = false;
  }
};

const selectCollection = async (collection: CollectionView) => {
  selectedCollectionId.value = collection.id;
  await loadItems(collection.id, false);
  await loadCollectionTags(collection.id);
  await seekMindApi.recordRecentView("collection", collection.id, collection.name, collection.description || "");
};

const openCreateCollectionDialog = () => {
  collectionDialogMode.value = "create";
  resetCollectionDialog();
  collectionDialogVisible.value = true;
  console.info("[SeekMind] collection dialog opened", {
    mode: "create",
    selectedCollectionId: selectedCollectionId.value || null,
  });
};

const openEditCollectionDialog = (collection: CollectionView) => {
  selectedCollectionId.value = collection.id;
  collectionDialogMode.value = "edit";
  syncCollectionDialogFromCollection(collection);
  collectionDialogVisible.value = true;
  console.info("[SeekMind] collection dialog opened", {
    mode: "edit",
    collectionId: collection.id,
    collectionName: collection.name,
  });
};

const closeCollectionDialog = () => {
  collectionDialogVisible.value = false;
  collectionSaving.value = false;
  resetCollectionDialog();
};

const submitCollectionDialog = async () => {
  const name = collectionName.value.trim();
  if (!name || collectionSaving.value) {
    return;
  }

  collectionSaving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    if (collectionDialogMode.value === "edit" && selectedCollectionId.value) {
      const updated = await seekMindApi.updateCollection(selectedCollectionId.value, {
        name,
        description: collectionDescription.value.trim(),
      });
      collections.value = collections.value.map((item) => (item.id === updated.id ? updated : item));
      selectedCollectionId.value = updated.id;
      infoMessage.value = t("page.collections.updated", { name: updated.name });
    } else {
      const created = await seekMindApi.createCollection(name, collectionDescription.value.trim());
      collections.value = [created, ...collections.value.filter((item) => item.id !== created.id)];
      await selectCollection(created);
      infoMessage.value = t("page.collections.created", { name: created.name });
    }
    collectionDialogVisible.value = false;
    resetCollectionDialog();
    console.info("[SeekMind] collection dialog submitted", {
      mode: collectionDialogMode.value,
      collectionName: name,
    });
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.saveCollection"));
  } finally {
    collectionSaving.value = false;
  }
};

const deleteCollection = async (collection: CollectionView) => {
  if (!window.confirm(t("page.collections.confirmDelete", { name: collection.name }))) {
    return;
  }

  try {
    await seekMindApi.deleteCollection(collection.id);
    collections.value = collections.value.filter((item) => item.id !== collection.id);
    if (selectedCollectionId.value === collection.id) {
      const next = collections.value[0] ?? null;
      if (next) {
        await selectCollection(next);
      } else {
        selectedCollectionId.value = "";
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
      }
    }
    infoMessage.value = t("page.collections.deleted", { name: collection.name });
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.deleteCollection"));
  }
};

const selectItem = (item: CollectionItemView) => {
  selectedItemId.value = item.id;
  itemNoteDraft.value = item.note;
  showDetailPanel.value = true;
  void loadItemTags(item.id);
};

const closeSelectedItem = () => {
  console.debug("[SeekMind] collection detail panel closed", {
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
    const updated = await seekMindApi.updateCollectionItemNote(selectedItem.value.id, {
      note: itemNoteDraft.value,
    });
    collectionItems.value = collectionItems.value.map((item) => (item.id === updated.id ? updated : item));
    selectedItemId.value = updated.id;
    itemNoteDraft.value = updated.note;
    infoMessage.value = t("page.collections.noteSaved");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.saveItem"));
  } finally {
    itemSaving.value = false;
  }
};

const addCollectionTag = async () => {
  if (!selectedCollectionId.value || !collectionTagName.value.trim()) {
    return;
  }

  try {
    await seekMindApi.addTagToTarget("collection", selectedCollectionId.value, collectionTagName.value);
    collectionTagName.value = "";
    await loadCollectionTags(selectedCollectionId.value);
    infoMessage.value = t("page.collections.tagAdded");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.tagSave"));
  }
};

const addItemTag = async () => {
  if (!selectedItem.value || !itemTagName.value.trim()) {
    return;
  }

  try {
    await seekMindApi.addTagToTarget("collection_item", selectedItem.value.id, itemTagName.value);
    itemTagName.value = "";
    await loadItemTags(selectedItem.value.id);
    infoMessage.value = t("page.collections.tagAdded");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.tagSave"));
  }
};

const removeCollectionTag = async (tag: TagView) => {
  if (!selectedCollectionId.value) {
    return;
  }

  try {
    await seekMindApi.removeTagFromTarget("collection", selectedCollectionId.value, tag.id);
    await loadCollectionTags(selectedCollectionId.value);
    infoMessage.value = t("page.collections.tagRemoved");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.tagRemove"));
  }
};

const removeItemTag = async (tag: TagView) => {
  if (!selectedItem.value) {
    return;
  }

  try {
    await seekMindApi.removeTagFromTarget("collection_item", selectedItem.value.id, tag.id);
    await loadItemTags(selectedItem.value.id);
    infoMessage.value = t("page.collections.tagRemoved");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.tagRemove"));
  }
};

const removeItem = async (item: CollectionItemView) => {
  if (!window.confirm(t("page.collections.confirmRemoveItem", { name: item.title }))) {
    return;
  }

  try {
    await seekMindApi.removeCollectionItem(item.id);
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
      const updated = await seekMindApi.listCollections();
      collections.value = updated;
    }
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.removeItem"));
  }
};

const openItemFile = async (item: CollectionItemView) => {
  if (!item.path.trim()) return;
  await seekMindApi.openFile(item.path);
};

const quickLookItem = async (item: CollectionItemView) => {
  if (!item.path.trim()) return;
  await seekMindApi.quickLookFile(item.path);
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
    const savedPath = await seekMindApi.exportCollectionMarkdown(collection.id, path);
    infoMessage.value = t("page.collections.exportedMarkdown", { path: savedPath });
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.collections.error.exportFailed"));
  }
};

const collectionContextMenuItems = computed<ContextMenuItem[]>(() => {
  if (!collectionMenuTarget.value) {
    return [];
  }

  return [
    {
      key: "edit",
      label: t("page.collections.editCollection"),
      icon: Pencil,
      handler: () => {
        if (collectionMenuTarget.value) {
          void selectCollection(collectionMenuTarget.value);
          openEditCollectionDialog(collectionMenuTarget.value);
        }
      },
    },
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
  if (collections.value.length > 0 && !selectedCollectionId.value) {
    await selectCollection(collections.value[0]);
  }
});

onActivated(async () => {
  await loadCollections(false);
});
</script>

<template>
  <div class="m-3 flex h-full min-h-0 flex-col overflow-hidden bg-transparent text-primary">
    <!-- Knowledge library header is kept compact to match the flat desktop layout. -->
    <header class="flex items-center justify-between gap-4 px-3 pb-2 pt-1">
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <span class="inline-flex h-8 w-8 items-center justify-center rounded-[10px] bg-white/72 text-accent" aria-hidden="true">
            <BookMarked :size="17" />
          </span>
          <div class="min-w-0">
            <h1 class="truncate text-[16px] font-semibold leading-6 tracking-[-0.01em] text-primary">
              {{ t("page.collections.title") }}
            </h1>
            <p class="mt-0.5 truncate text-[12px] leading-5 text-muted">
              {{ t("page.collections.subtitle") }}
            </p>
          </div>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <button
          class="inline-flex items-center gap-2 rounded-full bg-white/72 px-3.5 py-1.5 text-sm text-secondary transition hover:text-primary"
          type="button"
          :disabled="collectionsLoading"
          @click="loadCollections(false)"
        >
          <RefreshCw :size="16" />
          {{ t("page.collections.resync") }}
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-full bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition hover:bg-accent-strong"
          type="button"
          :disabled="!selectedCollection"
          @click="selectedCollection && void exportCollectionMarkdown(selectedCollection)"
        >
          <FileDown :size="16" />
          {{ t("page.collections.exportMarkdown") }}
        </button>
      </div>
    </header>

    <SeekMindToast v-if="errorMessage" :message="errorMessage" tone="error" />
    <SeekMindToast v-if="infoMessage" :message="infoMessage" tone="success" />

    <SplitPane class="min-h-0 flex-1" :panels="splitPanels">
      <template #left>
        <aside class="flex min-h-0 flex-1 flex-col overflow-hidden bg-[rgba(245,246,248,0.88)]">
          <div class="px-4 py-3">
            <div class="flex items-center gap-2">
              <div class="relative min-w-0 flex-1">
                <Search class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted" :size="15" />
                <input
                  v-model="collectionFilter"
                  class="w-full rounded-[14px] bg-white/72 py-2 pl-9 pr-3 text-sm text-primary placeholder:text-muted focus:outline-none"
                  type="text"
                  :placeholder="t('page.collections.filterPlaceholder')"
                >
              </div>
              <button
                class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-[14px] border border-default bg-white/72 text-secondary transition hover:bg-white/90 hover:text-primary"
                type="button"
                :title="t('page.collections.newCollection')"
                :aria-label="t('page.collections.newCollection')"
                @click="openCreateCollectionDialog"
              >
                <Plus :size="16" />
              </button>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            <div v-if="collectionsLoading" class="seekmind-elevated-card rounded-[18px] px-4 py-6 text-center text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else-if="filteredCollections.length === 0" class="seekmind-elevated-card rounded-[18px] px-4 py-6 text-center text-xs text-muted">
              {{ t("page.collections.emptyCollections") }}
            </div>
            <div v-else class="space-y-2">
              <div
                v-for="collection in filteredCollections"
                :key="collection.id"
                class="seekmind-elevated-card rounded-[14px] px-3 py-2.5 transition"
                :class="selectedCollectionId === collection.id ? 'seekmind-card-selected' : 'bg-white/72 hover:bg-white/90'"
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
                  <SeekMindBadge tone="default">{{ collection.item_count }}</SeekMindBadge>
                </div>
                <div class="mt-1.5 text-[11px] text-dim">{{ formatSeekMindDateOnly(collection.updated_at, locale.value) }}</div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <template #middle>
        <section class="seekmind-pane-center flex min-h-0 flex-1 flex-col overflow-hidden">
          <div class="flex items-start gap-3 px-4 pb-2 pt-3">
            <span class="card-icon seekmind-page-header-icon"><Layers3 :size="17" /></span>
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm font-semibold text-primary">
                {{ selectedCollection?.name || t("page.collections.noCollectionSelected") }}
              </div>
              <div class="mt-0.5 truncate text-[12px] text-muted">
                {{ selectedCollection?.description || t("page.collections.noDescription") }}
              </div>
            </div>
            <SeekMindBadge tone="default">{{ t("page.collections.itemCount", { count: collectionItems.length }) }}</SeekMindBadge>
          </div>
          <div v-if="selectedCollection" class="seekmind-content-block seekmind-content-block--tight-top">
            <!-- 标签输入改为单行标签栏：已有标签、加号和输入框始终在同一行。 -->
            <div class="seekmind-inline-tag-strip seekmind-elevated-card mt-2">
              <div
                v-for="tag in collectionTags"
                :key="tag.id"
                class="seekmind-inline-tag-chip text-[11px]"
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
              <button
                class="seekmind-inline-tag-add"
                type="button"
                :title="t('page.collections.addTag')"
                :aria-label="t('page.collections.addTag')"
                @click="focusCollectionTagInput"
              >
                <Plus :size="13" />
              </button>
              <input
                ref="collectionTagInputRef"
                v-model="collectionTagName"
                class="seekmind-inline-tag-input"
                type="text"
                :placeholder="t('page.collections.tagPlaceholder')"
                @keydown.enter.prevent="addCollectionTag"
              >
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto p-4 pt-3">
            <div v-if="!selectedCollection" class="seekmind-elevated-card rounded-[18px] px-4 py-8 text-center text-xs text-muted">
              {{ t("page.collections.emptyCollections") }}
            </div>
            <div v-else-if="itemsLoading" class="seekmind-elevated-card rounded-[18px] px-4 py-8 text-center text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else class="space-y-3">
              <div v-if="availableItemTagFilters.length > 0" class="seekmind-elevated-card rounded-[18px] px-3 py-3">
                <div class="flex items-center justify-between gap-2">
                  <div class="text-[11px] font-medium text-dim">
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
                    class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs transition"
                    :class="itemTagFilter === tag.id ? 'seekmind-card-selected text-primary' : 'bg-white/72 text-secondary hover:bg-white/90 hover:text-primary'"
                    type="button"
                    @click="toggleItemTagFilter(tag.id)"
                  >
                    <span class="max-w-[8rem] truncate">{{ tag.name }}</span>
                    <SeekMindBadge tone="default">{{ tag.count }}</SeekMindBadge>
                  </button>
                </div>
              </div>
              <div v-if="filteredCollectionItems.length === 0" class="seekmind-elevated-card rounded-[18px] px-4 py-8 text-center text-xs text-muted">
                {{ itemTagFilter ? t("page.collections.noFilteredItems") : t("page.collections.emptyItems") }}
              </div>
              <article
                v-else
                v-for="item in filteredCollectionItems"
                :key="item.id"
                class="seekmind-elevated-card cursor-pointer rounded-[18px] p-3 transition"
                :class="selectedItemId === item.id ? 'seekmind-card-selected' : 'bg-white/72 hover:bg-white/90'"
                @click="selectItem(item)"
                @contextmenu.prevent="openItemMenu(item, $event)"
              >
                <div class="flex items-start gap-3">
                  <SeekMindFileIcon :ext="(item.path.split('.').pop() || item.item_type).slice(0, 8)" />
                  <div class="min-w-0 flex-1">
                    <div class="flex items-start justify-between gap-2">
                      <div class="min-w-0">
                        <div class="truncate text-sm font-medium text-primary">{{ item.title }}</div>
                        <div class="mt-1 text-[11px] text-muted">{{ item.path || t("common.none") }}</div>
                      </div>
                      <SeekMindBadge :tone="itemTypeTone(item)">{{ itemTypeLabel(item) }}</SeekMindBadge>
                    </div>
                    <div v-if="item.title_path" class="mt-2 text-[11px] text-dim">
                      {{ t("page.collections.location") }}：{{ item.title_path }}
                    </div>
                    <div class="mt-2 max-h-[4.5rem] overflow-hidden text-sm leading-6 text-secondary">
                      {{ item.snippet || t("page.collections.noSnippet") }}
                    </div>
                    <div class="mt-2 flex flex-wrap gap-2 text-[11px] text-dim">
                      <span>{{ formatSeekMindDateOnly(item.created_at, locale.value) }}</span>
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
        <aside class="seekmind-pane-detail flex min-h-0 flex-1 flex-col overflow-hidden">
          <SeekMindDetailPanel v-if="selectedItem">
            <template #header>
              <div class="flex items-start gap-3">
                <span class="card-icon seekmind-page-header-icon"><BookMarked :size="17" /></span>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-semibold text-primary">{{ t("page.collections.detailTitle") }}</div>
                  <div class="mt-0.5 text-[12px] text-muted">{{ t("page.collections.detailDesc") }}</div>
                </div>
                <button
                  class="seekmind-close-button shrink-0"
                  type="button"
                  :title="t('common.close')"
                  @click="closeSelectedItem"
                >
                  <X :size="13" stroke-width="2.25" />
                </button>
              </div>
            </template>

            <SeekMindDetailSection :title="t('common.overview')" :subtitle="selectedItem.path || t('common.none')">
              <div class="flex items-start gap-3">
                <span class="card-icon seekmind-page-header-icon">
                  <component :is="itemTypeIcon(selectedItem)" :size="17" />
                </span>
                <div class="min-w-0 flex-1">
                  <div class="text-base font-semibold text-primary">{{ selectedItem.title }}</div>
                  <div v-if="selectedItem.title_path" class="mt-1 break-all text-sm leading-6 text-secondary">{{ selectedItem.title_path }}</div>
                </div>
                <SeekMindBadge :tone="itemTypeTone(selectedItem)">{{ itemTypeLabel(selectedItem) }}</SeekMindBadge>
              </div>
              <div class="grid grid-cols-2 gap-x-4 gap-y-2 text-xs leading-5 text-muted">
                <div>{{ t("page.collections.createdAt") }}：{{ formatSeekMindDateOnly(selectedItem.created_at, locale.value) }}</div>
                <div>{{ t("page.collections.updatedAt") }}：{{ formatSeekMindDateOnly(selectedItem.updated_at, locale.value) }}</div>
                <div class="break-all">{{ t("page.collections.collectionId") }}：{{ selectedItem.collection_id }}</div>
                <div class="break-all">{{ t("page.collections.sourceMeta") }}：{{ selectedItem.source_meta_json || t("common.none") }}</div>
              </div>
            </SeekMindDetailSection>

            <SeekMindDetailSection :title="t('common.originalText')">
              <div class="whitespace-pre-wrap text-sm leading-7 text-primary">
                {{ selectedItem.snippet || t("page.collections.noSnippet") }}
              </div>
            </SeekMindDetailSection>

            <SeekMindDetailSection :title="t('common.context')">
              <div class="seekmind-inline-tag-strip seekmind-elevated-card mt-2">
                <div
                  v-for="tag in itemTags"
                  :key="tag.id"
                  class="seekmind-inline-tag-chip text-[11px]"
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
                <button
                  class="seekmind-inline-tag-add"
                  type="button"
                  :title="t('page.collections.addTag')"
                  :aria-label="t('page.collections.addTag')"
                  @click="focusItemTagInput"
                >
                  <Plus :size="13" />
                </button>
                <input
                  ref="itemTagInputRef"
                  v-model="itemTagName"
                  class="seekmind-inline-tag-input"
                  type="text"
                  :placeholder="t('page.collections.tagPlaceholder')"
                  @keydown.enter.prevent="addItemTag"
                >
              </div>
              <div class="mt-3">
                <div class="mb-2 text-[11px] font-medium text-dim">{{ t("page.collections.note") }}</div>
                <textarea
                  v-model="itemNoteDraft"
                  class="min-h-[160px] w-full resize-y rounded-[14px] bg-white/72 px-3 py-2 text-sm text-primary placeholder:text-muted focus:outline-none"
                  :placeholder="t('page.collections.notePlaceholder')"
                />
                <div class="mt-3 flex items-center gap-2">
                  <button
                    class="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-strong disabled:cursor-not-allowed disabled:opacity-60"
                    type="button"
                    :disabled="itemSaving"
                    @click="saveItemNote"
                  >
                    <Pencil :size="16" />
                    {{ t("page.collections.saveNote") }}
                  </button>
                </div>
              </div>
            </SeekMindDetailSection>
          </SeekMindDetailPanel>
          <div v-else class="flex min-h-0 flex-1 items-center justify-center px-4 text-center text-xs text-muted">
            {{ t("page.collections.noItemSelected") }}
          </div>
        </aside>
      </template>
    </SplitPane>

    <SeekMindContextMenu
      v-if="collectionMenuVisible"
      :x="collectionMenuPosition.x"
      :y="collectionMenuPosition.y"
      :items="collectionContextMenuItems"
      @close="collectionMenuVisible = false"
    />
    <SeekMindContextMenu
      v-if="itemMenuVisible"
      :x="itemMenuPosition.x"
      :y="itemMenuPosition.y"
      :items="itemContextMenuItems"
      @close="itemMenuVisible = false"
    />

    <div
      v-if="collectionDialogVisible"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4 py-6"
      @click.self="closeCollectionDialog"
    >
      <div class="w-full max-w-[420px] rounded-[20px] border border-default bg-panel p-4 shadow-[0_18px_50px_rgba(15,23,42,0.14)]">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="text-sm font-semibold text-primary">
              {{ collectionDialogMode === "edit" ? t("page.collections.editCollection") : t("page.collections.newCollection") }}
            </div>
          </div>
          <button
            class="seekmind-close-button shrink-0"
            type="button"
            :title="t('common.close')"
            @click="closeCollectionDialog"
          >
            <X :size="13" stroke-width="2.25" />
          </button>
        </div>

        <div class="mt-4 space-y-2">
          <input
            v-model="collectionName"
            class="w-full rounded-[14px] bg-surface px-3 py-2 text-sm text-primary placeholder:text-muted focus:outline-none"
            type="text"
            :placeholder="t('page.collections.namePlaceholder')"
            @keydown.enter.prevent="submitCollectionDialog"
          >
          <textarea
            v-model="collectionDescription"
            class="min-h-[88px] w-full resize-none rounded-[14px] bg-surface px-3 py-2 text-sm text-primary placeholder:text-muted focus:outline-none"
            :placeholder="t('page.collections.descriptionPlaceholder')"
          />
        </div>

        <div class="mt-4 flex items-center justify-end gap-2">
          <button
            class="inline-flex items-center gap-2 rounded-full px-3 py-2 text-sm text-muted transition hover:text-primary"
            type="button"
            @click="closeCollectionDialog"
          >
            {{ t("common.cancel") }}
          </button>
          <button
            class="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-strong disabled:cursor-not-allowed disabled:opacity-60"
            type="button"
            :disabled="collectionSaving || !collectionName.trim()"
            @click="submitCollectionDialog"
          >
            <component :is="collectionDialogMode === 'edit' ? Pencil : FolderPlus" :size="15" />
            {{ collectionDialogMode === "edit" ? t("page.collections.saveChanges") : t("page.collections.create") }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
