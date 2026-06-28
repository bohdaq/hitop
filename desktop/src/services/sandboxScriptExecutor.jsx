// Desktop app: all script execution goes through the Rust backend (boa_engine).
// This stub keeps App.js imports happy — the sandbox iframe is never used.
const sandboxExecutor = {
  executeScript: async () => ({ contextUpdates: {} }),
};

export default sandboxExecutor;
