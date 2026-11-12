// CodeMirror setup for <hitop-code-editor>; loaded on first use, so the
// request view doesn't wait for it.
import { EditorState, Compartment } from "@codemirror/state";
import { Decoration, EditorView, MatchDecorator, ViewPlugin, hoverTooltip, keymap, lineNumbers, highlightActiveLine, drawSelection, placeholder } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { HighlightStyle, bracketMatching, indentOnInput, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { javascript } from "@codemirror/lang-javascript";
import { json, jsonParseLinter } from "@codemirror/lang-json";
import { linter, lintGutter } from "@codemirror/lint";
import { tags as t } from "@lezer/highlight";

// Colours from theme.css, so light and dark both work.
export const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.controlKeyword, t.definitionKeyword, t.moduleKeyword], color: "var(--md-sys-color-primary)" },
  { tag: [t.string, t.special(t.string)], color: "#9fcf86" },
  { tag: [t.number, t.bool, t.null], color: "#f2a29c" },
  { tag: [t.propertyName], color: "#e9c07a" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--md-sys-color-on-surface-variant)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "#8cc8f2" },
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


// UX2: ${variables} in a body, marked as the URL field marks them (amber, red and dashed when nothing
// defines one); hovering one shows its value and where it comes from. `resolve(name)` →
// {value, scope, missingSecret?} | null (the store's variableSource; secrets come masked).
const VAR = /\$\{([^}]*)\}/g;
export function variableMarks(resolve) {
  const marks = new MatchDecorator({
    regexp: VAR,
    decoration: (m) => {
      const found = resolve(m[1].trim());
      return Decoration.mark({ class: !found || found.missingSecret ? "cm-var cm-var-missing" : "cm-var" });
    },
  });
  const plugin = ViewPlugin.fromClass(
    class {
      constructor(view) {
        this.decorations = marks.createDeco(view);
      }
      update(u) {
        this.decorations = marks.updateDeco(u, this.decorations);
      }
    },
    { decorations: (v) => v.decorations },
  );
  const peek = hoverTooltip((view, pos) => {
    const line = view.state.doc.lineAt(pos);
    for (const m of line.text.matchAll(VAR)) {
      const from = line.from + m.index;
      const to = from + m[0].length;
      if (pos < from || pos > to) continue;
      const name = m[1].trim();
      const found = resolve(name);
      return {
        pos: from,
        end: to,
        above: true,
        create() {
          const dom = document.createElement("div");
          dom.className = "cm-var-peek";
          dom.textContent = !found ? `\${${name}}: nothing defines it` : found.missingSecret ? `\${${name}}: a secret with no value on this device (${found.scope})` : `\${${name}} = ${found.value || "(empty)"} · ${found.scope}`;
          return { dom };
        },
      };
    }
    return null;
  });
  return [plugin, peek];
}

export { EditorState, Compartment, EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection, placeholder, defaultKeymap, history, historyKeymap, indentWithTab, bracketMatching, indentOnInput, syntaxHighlighting };
