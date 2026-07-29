import { autocompletion, type Completion, type CompletionContext } from "@codemirror/autocomplete";

export type LumoraCompletion = Completion & {
  permission?: string;
};

const LUMORA_METHODS: LumoraCompletion[] = [
  {
    label: "log",
    type: "method",
    detail: "(level, message)",
    info: "Write a line to plugin run history. level: info | warn | error",
  },
  {
    label: "getAssets",
    type: "method",
    detail: "(ids: string[])",
    info: "Load selected assets (path, metadata). Requires read:assets.",
    permission: "read:assets",
  },
  {
    label: "renameAsset",
    type: "method",
    detail: "(id: string, newFileName: string)",
    info: "Rename a file on disk. Requires rename:filesystem.",
    permission: "rename:filesystem",
  },
  {
    label: "setRating",
    type: "method",
    detail: "(id: string, rating: number)",
    info: "Set star rating (0–5). Requires write:metadata.",
    permission: "write:metadata",
  },
  {
    label: "setTags",
    type: "method",
    detail: "(id: string, tags: string[])",
    info: "Replace asset tags. Requires write:metadata.",
    permission: "write:metadata",
  },
  {
    label: "moveAssets",
    type: "method",
    detail: "(ids: string[], destDir: string)",
    info: "Move files into a folder. Requires move:filesystem.",
    permission: "move:filesystem",
  },
  {
    label: "copyAssets",
    type: "method",
    detail: "(ids: string[], destDir: string)",
    info: "Copy files (Milestone 3). Requires copy:filesystem.",
    permission: "copy:filesystem",
  },
  {
    label: "exportAssets",
    type: "method",
    detail: "(ids: string[], options?)",
    info: "Export resized copies. Requires export:assets.",
    permission: "export:assets",
  },
  {
    label: "organizeAssets",
    type: "method",
    detail: "(ids: string[], template: string)",
    info: "Organize into folder structure (Milestone 3).",
    permission: "move:filesystem",
  },
  {
    label: "createFolder",
    type: "method",
    detail: "(path: string)",
    info: "Create a folder on disk (Milestone 3).",
    permission: "move:filesystem",
  },
];

const CONTEXT_MEMBERS: LumoraCompletion[] = [
  {
    label: "actionId",
    type: "property",
    detail: "string",
    info: "Id of the action being run (from manifest).",
  },
  {
    label: "assetIds",
    type: "property",
    detail: "string[]",
    info: "Ids of the currently selected photos.",
  },
  {
    label: "mode",
    type: "property",
    detail: '"preview" | "apply"',
    info: "Run mode — preview shows a plan, apply executes changes.",
  },
  {
    label: "libraryId",
    type: "property",
    detail: "string",
    info: "Active library id (usually default).",
  },
  {
    label: "reportProgress",
    type: "method",
    detail: "(current: number, total: number)",
    info: "Update the progress dialog during long operations.",
  },
];

const TOP_LEVEL: LumoraCompletion[] = [
  {
    label: "lumora",
    type: "namespace",
    detail: "Lumora host API",
    info: "Sandboxed API for reading/writing assets. Type lumora. for methods.",
  },
  {
    label: "context",
    type: "variable",
    detail: "RunContext",
    info: "Selection context passed to runAction. Type context. for members.",
  },
  {
    label: "runAction",
    type: "function",
    detail: "async (actionId, context)",
    info: "Required entry point — export async function runAction(...).",
  },
];

function memberCompletions(
  from: number,
  items: LumoraCompletion[],
  filterText: string,
): { from: number; options: LumoraCompletion[] } {
  const lower = filterText.toLowerCase();
  const options = items.filter((item) =>
    item.label.toLowerCase().startsWith(lower),
  );
  return { from, options };
}

function lumoraAutocomplete(context: CompletionContext) {
  const lumoraDot = context.matchBefore(/lumora\.\w*/);
  if (lumoraDot) {
    const prefix = lumoraDot.text.slice("lumora.".length);
    return {
      ...memberCompletions(lumoraDot.from + "lumora.".length, LUMORA_METHODS, prefix),
      validFor: /^\w*$/,
    };
  }

  const contextDot = context.matchBefore(/context\.\w*/);
  if (contextDot) {
    const prefix = contextDot.text.slice("context.".length);
    return {
      ...memberCompletions(contextDot.from + "context.".length, CONTEXT_MEMBERS, prefix),
      validFor: /^\w*$/,
    };
  }

  const word = context.matchBefore(/\b[\w$]*/);
  if (!word || (word.from === word.to && !context.explicit)) {
    return null;
  }

  const prefix = word.text;
  const options = TOP_LEVEL.filter((item) =>
    item.label.toLowerCase().startsWith(prefix.toLowerCase()),
  );
  if (options.length === 0) return null;

  return { from: word.from, options, validFor: /^\w*$/ };
}

export const lumoraAutocompleteExtension = autocompletion({
  override: [lumoraAutocomplete],
  activateOnTyping: true,
  defaultKeymap: true,
  closeOnBlur: true,
});

export { LUMORA_METHODS, CONTEXT_MEMBERS };
