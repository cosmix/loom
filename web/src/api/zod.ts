import { z } from "zod";

/// The dashboard's CSP (`loom/src/commands/status/web/http.rs`) ships no
/// `script-src`, so zod's `new Function("")` eval probe (guarding its JIT
/// fast path for object schemas) logs a `securitypolicyviolation` on every
/// load even though the throw it triggers is caught. Setting `jitless`
/// before any schema is built skips the probe entirely, which is why every
/// schema module imports `z` from here instead of from `"zod"` directly.
z.config({ jitless: true });

export { z };
