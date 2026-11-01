// graphql-js for the GraphQL body editor: the introspection query and the
// schema built from its answer. Loaded on the first schema fetch.
import { buildClientSchema, getIntrospectionQuery } from "graphql";

export const INTROSPECTION_QUERY = getIntrospectionQuery();

// A schema from an introspection response's text: `{data: {__schema}}`
// (or just `{__schema}`). Throws with what the server said otherwise.
export function schemaFromResponse(text) {
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    throw new Error(`the answer isn't JSON: ${text.slice(0, 120)}`);
  }
  const data = json?.data ?? json;
  if (!data?.__schema) {
    const errors = (json?.errors || []).map((e) => e?.message).filter(Boolean);
    throw new Error(errors.length ? errors.join("; ") : "the answer has no __schema (is introspection turned off?)");
  }
  return buildClientSchema(data);
}
