import { useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { EditorView } from "@codemirror/view";
import { lumoraAutocompleteExtension } from "./lumoraApiCompletions";

const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    maxHeight: "100%",
    fontSize: "13px",
  },
  ".cm-scroller": {
    fontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
    lineHeight: "1.6",
  },
  ".cm-content": {
    minHeight: "100%",
  },
  ".cm-gutters": {
    backgroundColor: "#1e1e1e",
    borderRight: "1px solid #2d2d2d",
    color: "#6e7681",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "#2a2d2e",
    color: "#c6cdd5",
  },
  "&.cm-focused .cm-cursor": {
    borderLeftColor: "#2f6f5e",
  },
  "&.cm-focused .cm-selectionBackground, ::selection": {
    backgroundColor: "rgba(47, 111, 94, 0.35) !important",
  },
  ".cm-activeLine": {
    backgroundColor: "rgba(255, 255, 255, 0.04)",
  },
  ".cm-tooltip-autocomplete": {
    border: "1px solid #3c3c3c",
    borderRadius: "6px",
    backgroundColor: "#252526",
    boxShadow: "0 8px 24px rgba(0, 0, 0, 0.45)",
    fontSize: "12px",
  },
});

export function PluginCodeEditor({
  value,
  onChange,
  readOnly = false,
}: {
  value: string;
  onChange: (value: string) => void;
  readOnly?: boolean;
}) {
  const extensions = useMemo(
    () => [javascript({ jsx: false, typescript: false }), lumoraAutocompleteExtension, editorTheme],
    [],
  );

  return (
    <div className="plugin-codemirror-wrap">
      <CodeMirror
        value={value}
        height="100%"
        theme="dark"
        extensions={extensions}
        onChange={onChange}
        readOnly={readOnly}
        basicSetup={{
          lineNumbers: true,
          highlightActiveLineGutter: true,
          highlightActiveLine: true,
          foldGutter: true,
          dropCursor: true,
          allowMultipleSelections: true,
          indentOnInput: true,
          bracketMatching: true,
          closeBrackets: true,
          autocompletion: false,
          rectangularSelection: true,
          crosshairCursor: false,
          highlightSelectionMatches: true,
          tabSize: 2,
        }}
        placeholder="// export async function runAction(actionId, context) { ... }"
      />
    </div>
  );
}
