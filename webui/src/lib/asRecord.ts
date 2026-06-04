/** Narrow an `unknown` to a plain object by a runtime check, returning an empty
 *  object for null/non-object inputs. Lets callers read fields off untrusted
 *  event payloads without throwing or blind `as` casts. */
export function asRecord(v: unknown): Record<string, unknown> {
  return v != null && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}
