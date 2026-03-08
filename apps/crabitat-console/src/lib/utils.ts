export interface PromptTreeNode {
  name: string;
  fullPath?: string;
  children?: PromptTreeNode[];
}

export function buildPromptTree(paths: string[]): PromptTreeNode[] {
  const root: PromptTreeNode[] = [];

  for (const path of paths) {
    const parts = path.split("/");
    let currentLevel = root;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isFile = i === parts.length - 1;
      let node = currentLevel.find((n) => n.name === part);

      if (!node) {
        node = { name: part };
        if (isFile) {
          node.fullPath = path;
        } else {
          node.children = [];
        }
        currentLevel.push(node);
        // Sort: directories first, then alphabetically
        currentLevel.sort((a, b) => {
          if (!!a.children !== !!b.children) {
            return a.children ? -1 : 1;
          }
          return a.name.localeCompare(b.name);
        });
      }
      if (node.children) {
        currentLevel = node.children;
      }
    }
  }

  return root;
}
