"use client";

// Browsable file tree of an archived repository. Renders the preserved
// snapshot's tree (from `/archive/{id}/tree`), lets users expand directories,
// and opens text files in a built-in viewer served from `/archive/{id}/blob`.

import { useEffect, useMemo, useState } from "react";
import {
  Folder,
  FolderOpen,
  FileCode2,
  ChevronRight,
  Loader2,
  FileWarning,
  GitCommitHorizontal,
} from "lucide-react";
import { api } from "@/lib/api";
import { formatBytes, type TreeEntry } from "@/lib/types";

interface TreeNode {
  name: string;
  path: string;
  type: "dir" | "file";
  size?: number | null;
  children?: TreeNode[];
}

function buildTree(entries: TreeEntry[]): TreeNode[] {
  const dirs = new Map<string, TreeNode>();
  const root: TreeNode[] = [];

  for (const e of entries) {
    if (e.type !== "blob") continue;
    const parts = e.path.split("/");
    const fileName = parts.pop()!;
    let parent = root;
    let currentPath = "";
    for (const part of parts) {
      currentPath = currentPath ? `${currentPath}/${part}` : part;
      let node = dirs.get(currentPath);
      if (!node) {
        node = { name: part, path: currentPath, type: "dir", children: [] };
        dirs.set(currentPath, node);
        parent.push(node);
      }
      parent = node.children!;
    }
    parent.push({ name: fileName, path: e.path, type: "file", size: e.size });
  }

  const sortNodes = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.type !== b.type) return a.type === "dir" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const n of nodes) if (n.children) sortNodes(n.children);
  };
  sortNodes(root);
  return root;
}

const MAX_TEXT_BYTES = 512 * 1024;

export function FileTree({ archiveId }: { archiveId: string }) {
  const [entries, setEntries] = useState<TreeEntry[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [openPath, setOpenPath] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [binary, setBinary] = useState(false);
  const [contentLoading, setContentLoading] = useState(false);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);
    setEntries(null);
    setOpenPath(null);
    setContent(null);
    api
      .archiveTree(archiveId)
      .then((res) => {
        if (!active) return;
        setEntries(res.entries);
        setExpanded(new Set(res.entries.map((e) => e.path.split("/").slice(0, -1).join("/")).filter(Boolean)));
      })
      .catch((e: Error) => active && setError(e.message))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [archiveId]);

  const tree = useMemo(() => buildTree(entries ?? []), [entries]);

  const openFile = async (node: TreeNode) => {
    setOpenPath(node.path);
    setContent(null);
    setBinary(false);

    if (node.size != null && node.size > MAX_TEXT_BYTES) {
      setBinary(true);
      return;
    }

    setContentLoading(true);
    try {
      const res = await fetch(api.blobUrl(archiveId, node.path), { cache: "no-store" });
      if (!res.ok) {
        setBinary(true);
        return;
      }
      const contentType = res.headers.get("content-type") ?? "";
      const text = await res.text();
      const looksText =
        contentType.includes("text/") || contentType.includes("json") || contentType.includes("yaml");
      if (!looksText && text.includes("\u0000")) {
        setBinary(true);
      } else {
        setContent(text);
      }
    } catch {
      setBinary(true);
    } finally {
      setContentLoading(false);
    }
  };

  const toggle = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const renderNodes = (nodes: TreeNode[], depth: number) =>
    nodes.map((node) => {
      if (node.type === "dir") {
        const isOpen = expanded.has(node.path);
        return (
          <div key={node.path}>
            <button
              onClick={() => toggle(node.path)}
              className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-sm transition-colors hover:bg-panel-2/60"
              style={{ paddingLeft: `${depth * 14 + 8}px` }}
            >
              <ChevronRight className={`size-3.5 shrink-0 text-ink-faint transition-transform ${isOpen ? "rotate-90" : ""}`} />
              {isOpen ? (
                <FolderOpen className="size-4 shrink-0 text-violet" />
              ) : (
                <Folder className="size-4 shrink-0 text-violet" />
              )}
              <span className="truncate text-ink-dim">{node.name}</span>
            </button>
            {isOpen && renderNodes(node.children ?? [], depth + 1)}
          </div>
        );
      }

      return (
        <button
          key={node.path}
          onClick={() => openFile(node)}
          className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-sm transition-colors hover:bg-panel-2/60"
          style={{ paddingLeft: `${depth * 14 + 26}px` }}
        >
          <FileCode2 className="size-4 shrink-0 text-ink-faint" />
          <span className={`truncate ${openPath === node.path ? "text-neon" : "text-ink"}`}>{node.name}</span>
          {node.size != null && (
            <span className="ml-auto shrink-0 text-[10px] text-ink-faint">{formatBytes(node.size)}</span>
          )}
        </button>
      );
    });

  return (
    <div className="glass mt-6 overflow-hidden rounded-2xl">
      <div className="flex items-center justify-between gap-2 border-b border-edge px-5 py-3">
        <div className="flex items-center gap-2">
          <FolderOpen className="size-4 text-violet" />
          <p className="text-xs uppercase tracking-wider text-ink-faint">Preserved files</p>
        </div>
        <span className="font-mono text-[10px] text-ink-faint">
          {entries ? `${entries.length.toLocaleString()} entries` : ""}
        </span>
      </div>

      {loading && (
        <div className="space-y-2 p-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="h-7 animate-pulse rounded-md bg-panel-2/60" />
          ))}
        </div>
      )}

      {error && (
        <p className="p-5 text-sm text-pink">
          Unable to load the preserved file tree — the archive may predate tree snapshots.
        </p>
      )}

      {!loading && !error && entries && entries.length === 0 && (
        <p className="p-5 text-sm text-ink-faint">
          No file tree was captured for this archive. Download the bundle to inspect it locally.
        </p>
      )}

      {!loading && !error && tree.length > 0 && (
        <div className="max-h-[360px] overflow-auto p-2 font-mono text-[13px]">
          {renderNodes(tree, 0)}
        </div>
      )}

      {openPath && (
        <div className="border-t border-edge">
          <div className="flex items-center justify-between gap-2 bg-panel/50 px-4 py-2">
            <p className="truncate font-mono text-xs text-neon">{openPath}</p>
            <button
              onClick={() => setOpenPath(null)}
              className="shrink-0 text-[10px] uppercase tracking-wider text-ink-faint transition-colors hover:text-pink"
            >
              close
            </button>
          </div>
          <div className="max-h-[420px] overflow-auto">
            {contentLoading && (
              <div className="flex items-center gap-2 p-5 text-sm text-ink-faint">
                <Loader2 className="size-4 animate-spin" /> Extracting from the preserved bundle…
              </div>
            )}
            {!contentLoading && binary && (
              <div className="flex items-center gap-3 p-5 text-sm text-ink-faint">
                <FileWarning className="size-5 text-amber" />
                Binary or oversized file — download the archive bundle to view it locally.
              </div>
            )}
            {!contentLoading && !binary && content !== null && (
              <pre className="whitespace-pre-wrap break-words p-4 font-mono text-[12.5px] leading-relaxed text-ink-dim">
                {content}
              </pre>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export function CommitRefTag({ commitRef }: { commitRef?: string | null }) {
  if (!commitRef) return null;
  return (
    <span className="flex items-center gap-1.5 font-mono text-xs text-ink-faint">
      <GitCommitHorizontal className="size-3.5" /> {commitRef.slice(0, 12)}
    </span>
  );
}
