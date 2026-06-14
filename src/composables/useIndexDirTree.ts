/**
 * @author MorningSun
 * @CreatedDate 2026/06/14
 * @Description 索引目录树构建与展开状态管理，兼容 Windows 和 macOS 路径展示与父子关系计算。
 */
import { computed, ref, unref, type MaybeRef } from "vue";
import type { IndexDirView } from "../types/SeekMind";

export interface IndexDirTreeNode {
  dir: IndexDirView;
  children: IndexDirTreeNode[];
  parentPath: string;
  normalizedPath: string;
  isVirtual: boolean;
}

export interface VisibleIndexDirRow {
  dir: IndexDirView;
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
  displayName: string;
  fullPath: string;
  parentPath: string;
  normalizedPath: string;
  isVirtual: boolean;
}

export interface UseIndexDirTreeOptions {
  virtualDirPrefix?: string;
}

const DEFAULT_VIRTUAL_DIR_PREFIX = "virtual://";

export const normalizeIndexDirPath = (path: string) =>
  path.replace(/\\/g, "/").replace(/\/+$/, "");

export const getIndexDirDisplayName = (path: string) => {
  const normalized = normalizeIndexDirPath(path);
  if (normalized.startsWith(DEFAULT_VIRTUAL_DIR_PREFIX)) {
    return normalized.slice(DEFAULT_VIRTUAL_DIR_PREFIX.length) || normalized;
  }

  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] || normalized;
};

const findParentPathFromPaths = (path: string, availablePaths: string[]) => {
  const normalized = normalizeIndexDirPath(path);
  const parent = availablePaths
    .filter((candidate) => candidate !== normalized && normalized.startsWith(`${candidate}/`))
    .sort((left, right) => right.length - left.length)[0];

  return parent ?? "";
};

export function useIndexDirTree(
  items: MaybeRef<IndexDirView[]>,
  options: UseIndexDirTreeOptions = {},
) {
  const expandedPaths = ref<Record<string, boolean>>({});
  const virtualDirPrefix = options.virtualDirPrefix ?? DEFAULT_VIRTUAL_DIR_PREFIX;
  const allPaths = computed(() =>
    unref(items)
      .map((item) => normalizeIndexDirPath(item.path))
      .sort((left, right) => left.length - right.length),
  );

  const buildTree = (nodes: IndexDirView[]) => {
    const nodeMap = new Map<string, IndexDirTreeNode>();
    for (const item of nodes) {
      const normalized = normalizeIndexDirPath(item.path);
      nodeMap.set(normalized, {
        // 修复：目录树内部统一使用 `/` 路径计算父子关系，
        // 但动作仍必须保留原始路径，避免 Windows 下删除/启停/重建传错路径。
        dir: item,
        children: [],
        parentPath: "",
        normalizedPath: normalized,
        isVirtual: normalized.startsWith(virtualDirPrefix),
      });
    }

    const roots: IndexDirTreeNode[] = [];
    const availablePaths = [...nodeMap.keys()];

    for (const node of nodeMap.values()) {
      const parentPath = findParentPathFromPaths(node.normalizedPath, availablePaths);
      node.parentPath = parentPath;

      const parent = parentPath ? nodeMap.get(parentPath) ?? null : null;
      if (parent) {
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    const sortNodes = (list: IndexDirTreeNode[]) => {
      list.sort((left, right) => {
        const leftVirtual = left.isVirtual ? 1 : 0;
        const rightVirtual = right.isVirtual ? 1 : 0;
        if (leftVirtual !== rightVirtual) {
          return leftVirtual - rightVirtual;
        }
        return getIndexDirDisplayName(left.normalizedPath).localeCompare(
          getIndexDirDisplayName(right.normalizedPath),
        );
      });
      for (const node of list) {
        sortNodes(node.children);
      }
    };

    sortNodes(roots);
    return roots;
  };

  const tree = computed(() => buildTree(unref(items)));

  const isExpanded = (path: string, depth: number, hasChildren: boolean) => {
    const normalized = normalizeIndexDirPath(path);
    if (expandedPaths.value[normalized] !== undefined) {
      return expandedPaths.value[normalized];
    }
    return depth === 0 && hasChildren;
  };

  const setExpanded = (path: string, expanded: boolean) => {
    const normalized = normalizeIndexDirPath(path);
    expandedPaths.value = {
      ...expandedPaths.value,
      [normalized]: expanded,
    };
  };

  const toggleExpanded = (path: string, depth: number, hasChildren: boolean) => {
    if (!hasChildren) {
      return;
    }
    setExpanded(path, !isExpanded(path, depth, hasChildren));
  };

  const expandAncestors = (path: string) => {
    let current = normalizeIndexDirPath(path);
    const next = { ...expandedPaths.value };

    while (current) {
      next[current] = true;
      const parent = findParentPathFromPaths(current, allPaths.value);
      if (!parent || parent === current) {
        break;
      }
      current = parent;
    }

    expandedPaths.value = next;
  };

  const visibleRows = computed<VisibleIndexDirRow[]>(() => {
    const rows: VisibleIndexDirRow[] = [];

    const walk = (nodes: IndexDirTreeNode[], depth: number) => {
      for (const node of nodes) {
        const hasChildren = node.children.length > 0;
        const expanded = isExpanded(node.normalizedPath, depth, hasChildren);

        rows.push({
          dir: node.dir,
          depth,
          hasChildren,
          expanded,
          displayName: getIndexDirDisplayName(node.normalizedPath),
          fullPath: node.dir.path,
          parentPath: node.parentPath,
          normalizedPath: node.normalizedPath,
          isVirtual: node.isVirtual,
        });

        if (hasChildren && expanded) {
          walk(node.children, depth + 1);
        }
      }
    };

    walk(tree.value, 0);
    return rows;
  });

  return {
    tree,
    visibleRows,
    expandedPaths,
    allPaths,
    isExpanded,
    setExpanded,
    toggleExpanded,
    expandAncestors,
    normalizeIndexDirPath,
    getIndexDirDisplayName,
  };
}
