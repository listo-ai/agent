import React from "react";
import type { HeadingNode } from "../types";

const tags: Record<number, string> = {
  1: "h1",
  2: "h2",
  3: "h3",
  4: "h4",
  5: "h5",
  6: "h6",
};

const sizeClass: Record<number, string> = {
  1: "text-2xl font-bold",
  2: "text-xl font-semibold",
  3: "text-lg font-semibold",
  4: "text-base font-semibold",
  5: "text-sm font-semibold",
  6: "text-xs font-semibold",
};

export function HeadingComponent({ node }: { node: HeadingNode }) {
  const level = node.level ?? 2;
  const Tag = (tags[level] ?? "h2") as React.ElementType;
  return <Tag className={sizeClass[level] ?? "text-xl font-semibold"}>{node.content}</Tag>;
}
