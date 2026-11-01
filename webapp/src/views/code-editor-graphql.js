// GraphQL for <hitop-code-editor language="graphql">: highlighting, completion
// and checks from a schema (cm6-graphql, graphql-js). Loaded only when a
// GraphQL body is shown, so other editors don't wait for it.
import { autocompletion } from "@codemirror/autocomplete";
import { linter, lintGutter } from "@codemirror/lint";
import { completion, getSchema, graphqlLanguageSupport, posToOffset, stateExtensions, updateSchema } from "cm6-graphql";
import { getDiagnostics } from "graphql-language-service";

// GraphQL: syntax errors, and errors against the schema once one is
// fetched. A query holding a ${variable} outside a string isn't GraphQL yet
// (it is filled in when sent), so it isn't checked.
const graphqlLinter = linter((view) => {
  const text = view.state.doc.toString();
  if (!text.trim() || text.includes("${")) return [];
  const doc = view.state.doc;
  const offset = (pos) => {
    try {
      return Math.min(posToOffset(doc, pos), doc.length);
    } catch {
      return doc.length;
    }
  };
  return getDiagnostics(text, getSchema(view.state) || undefined)
    .slice(0, 20)
    .map((d) => {
      const from = offset(d.range.start);
      return { from, to: Math.max(from, offset(d.range.end)), severity: d.severity === 1 ? "error" : "warning", message: d.message };
    });
});

// `schema`: a graphql-js GraphQLSchema for completion (fields, arguments,
// types), or null for syntax only.
export function graphqlExtensions(schema = null) {
  return [graphqlLanguageSupport(), completion, autocompletion(), stateExtensions(schema ?? undefined), graphqlLinter, lintGutter()];
}

export { updateSchema };
