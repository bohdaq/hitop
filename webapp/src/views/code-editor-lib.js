// CodeMirror setup for <hitop-code-editor>; loaded on first use, so the
// request view doesn't wait for it.
import { EditorState, Compartment } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection, placeholder } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { HighlightStyle, bracketMatching, indentOnInput, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { javascript } from "@codemirror/lang-javascript";
import { json, jsonParseLinter } from "@codemirror/lang-json";
import { linter, lintGutter } from "@codemirror/lint";
import { tags as t } from "@lezer/highlight";

// Colours from theme.css, so light and dark both work.
export const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.controlKeyword, t.definitionKeyword, t.moduleKeyword], color: "var(--md-sys-color-primary)" },
  { tag: [t.string, t.special(t.string)], color: "light-dark(#3f6b2a, #9fcf86)" },
  { tag: [t.number, t.bool, t.null], color: "light-dark(#a3221b, #f2a29c)" },
  { tag: [t.propertyName], color: "light-dark(#7a4b00, #e9c07a)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--md-sys-color-on-surface-variant)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "light-dark(#1f5f8b, #8cc8f2)" },
]);

export const theme = EditorView.theme({
  "&": {
    fontFamily: "var(--font-mono)",
    fontSize: "var(--text-sm)",
    color: "var(--md-sys-color-on-surface)",
    backgroundColor: "var(--md-sys-color-surface-container-lowest)",
    border: "1px solid var(--md-sys-color-outline)",
    borderRadius: "var(--md-sys-shape-corner-small)",
  },
  "&.cm-focused": { outline: "2px solid var(--md-sys-color-primary)", outlineOffset: "-1px" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "var(--leading-sm)", minHeight: "10lh", maxHeight: "60vh" },
  ".cm-content": { caretColor: "var(--md-sys-color-primary)", padding: "var(--space-75) 0" },
  ".cm-gutters": { backgroundColor: "transparent", border: "0", color: "var(--md-sys-color-on-surface-variant)" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--md-sys-color-primary) 6%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": { backgroundColor: "var(--md-sys-color-primary-container)" },
  ".cm-placeholder": { color: "var(--md-sys-color-on-surface-variant)" },
  ".cm-tooltip": { backgroundColor: "var(--md-sys-color-surface-container-high)", color: "var(--md-sys-color-on-surface)", border: "1px solid var(--md-sys-color-outline-variant)", borderRadius: "var(--md-sys-shape-corner-extra-small)" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--md-sys-color-primary-container)", color: "var(--md-sys-color-on-primary-container)" },
  ".cm-completionDetail": { color: "var(--md-sys-color-on-surface-variant)", fontStyle: "normal", marginLeft: "var(--space-100)" },
});

// JavaScript: report the parser's error nodes. Lezer recovers from errors,
// so this marks the spot without stopping highlighting.
const jsSyntaxLinter = linter((view) => {
  const found = [];
  syntaxTree(view.state).iterate({
    enter(node) {
      if (node.type.isError) {
        const line = view.state.doc.lineAt(node.from);
        found.push({ from: node.from, to: Math.max(node.to, node.from + 1), severity: "error", message: `Syntax error on line ${line.number}` });
      }
    },
  });
  return found.slice(0, 20);
});

export function languageExtensions(language) {
  if (language === "javascript") return [javascript(), jsSyntaxLinter, lintGutter()];
  if (language === "json") {
    // An empty body is fine; only report errors once there is text.
    const jsonLinter = linter((view) => (view.state.doc.length ? jsonParseLinter()(view) : []));
    return [json(), jsonLinter, lintGutter()];
  }
  return [];
}


export { EditorState, Compartment, EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection, placeholder, defaultKeymap, history, historyKeymap, indentWithTab, bracketMatching, indentOnInput, syntaxHighlighting };
