// ============================================================================
// Skill Studio - Zod jitless mode
// Imported first by main.tsx, before any module builds a Zod object schema.
// ============================================================================

import { z } from "zod";

// The release CSP forbids eval. Zod probes `Function("")` when it builds its
// first object schema, unless jitless is set, and the CSP reports that probe.
z.config({ jitless: true });
