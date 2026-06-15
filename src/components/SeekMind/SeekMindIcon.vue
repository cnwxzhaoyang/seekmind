<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/11
 * @Description SeekMind 统一图标组件，将前端资源目录中的 SVG 解析为 Vue 渲染节点，避免 Windows 下 raw SVG 注入不稳定。
 */
import { computed, defineComponent, h, type Component } from "vue";

defineOptions({
  name: "SeekMindIcon",
});

type SvgAstNode = {
  tag: string;
  props: Record<string, string>;
  children: SvgAstChild[];
};

type SvgAstChild = SvgAstNode | string;

const iconModules = import.meta.glob<string>("../../assets/seekmind-icons/svg/*.svg", {
  eager: true,
  query: "?raw",
  import: "default",
});

const parseSvgNode = (node: ChildNode): SvgAstChild | null => {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node.nodeValue ?? "";
    return text.trim() ? text : null;
  }

  if (node.nodeType !== Node.ELEMENT_NODE) {
    return null;
  }

  const element = node as Element;
  const props: Record<string, string> = {};
  for (const { name, value } of Array.from(element.attributes)) {
    props[name] = value;
  }

  const children = Array.from(element.childNodes)
    .map(parseSvgNode)
    .filter((child): child is SvgAstChild => child !== null);

  return {
    tag: element.tagName.toLowerCase(),
    props,
    children,
  };
};

const renderSvgNode = (node: SvgAstChild): ReturnType<typeof h> | string => {
  if (typeof node === "string") {
    return node;
  }

  return h(node.tag, node.props, node.children.map((child) => renderSvgNode(child)));
};

const createIconComponent = (svg: string): Component => {
  const parser = new DOMParser();
  const document = parser.parseFromString(svg, "image/svg+xml");
  const root = document.documentElement;
  const rootAst = parseSvgNode(root);

  if (!rootAst || typeof rootAst === "string" || rootAst.tag !== "svg") {
    return defineComponent({
      name: "SeekMindSvgIconEmpty",
      setup() {
        return () => null;
      },
    });
  }

  const rootProps = {
    ...rootAst.props,
    width: "100%",
    height: "100%",
  };

  return defineComponent({
    name: "SeekMindSvgIcon",
    setup() {
      return () => h(rootAst.tag, rootProps, rootAst.children.map((child) => renderSvgNode(child)));
    },
  });
};

const iconMap = Object.fromEntries(
  Object.entries(iconModules).map(([path, svg]) => {
    const match = path.match(/\/(icon-[^/]+)\.svg$/);
    return [match?.[1] ?? path, createIconComponent(svg)];
  }),
) as Record<string, Component>;

const props = withDefaults(defineProps<{
  icon: string;
  size?: number;
}>(), {
  size: 20,
});

const resolvedIcon = computed(() => iconMap[props.icon] ?? null);

const style = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
}));
</script>

<template>
  <span class="seekmind-icon-root" :style="style" aria-hidden="true">
    <component :is="resolvedIcon" v-if="resolvedIcon" />
  </span>
</template>

<style scoped>
.seekmind-icon-root {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: inherit;
  flex-shrink: 0;
}
</style>
