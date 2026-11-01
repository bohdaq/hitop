// The desktop's history.json holds hitop-core's HistoryItem (the CLI writes
// it too): snake_case fields and a millisecond timestamp. The web app's items
// map onto it; what Rust has no field for (headers, body, scripts) is kept
// as extra keys, which hitop-core preserves.
export function historyFromRust(items) {
  return (items || []).map(({ id, timestamp, url, method, status_code, success, collection_name, request_name, duration_ms, ...rest }) => ({
    headers: [],
    body: "",
    preRequestScript: "",
    postRequestScript: "",
    ...rest,
    id,
    timestamp: new Date(Number(timestamp) || 0).toISOString(),
    url,
    method,
    statusCode: status_code,
    success,
    ...(collection_name != null ? { collectionName: collection_name, isCollectionRun: rest.isCollectionRun ?? true } : {}),
    ...(request_name != null ? { requestName: request_name } : {}),
    ...(duration_ms ? { durationMs: duration_ms } : {}),
  }));
}

export function historyToRust(items) {
  return (items || []).map(({ id, timestamp, url, method, statusCode, success, collectionName, requestName, durationMs, ...rest }) => ({
    ...rest,
    id: Number(id) || 0,
    timestamp: Date.parse(timestamp) || 0,
    url: url ?? "",
    method: method ?? "GET",
    status_code: Number.isInteger(statusCode) && statusCode >= 0 && statusCode < 65536 ? statusCode : 0,
    success: Boolean(success),
    collection_name: collectionName ?? null,
    request_name: requestName ?? null,
    duration_ms: Math.max(0, Math.round(durationMs || 0)),
  }));
}

