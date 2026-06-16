// l10n — catalog lookup + interpolation. Message values are either a plain
// string (optionally with {name} placeholders) or a function that receives a
// single arg (used for counts / plurals, which differ per language). Keeping
// the runtime tiny avoids pulling in an ICU MessageFormat parser for two
// languages.

export type MessageArg = number | string | Record<string, string | number>;
export type MessageValue = string | ((arg?: MessageArg) => string);
export type Catalog = Record<string, MessageValue>;

function interpolate(template: string, params: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) =>
    key in params ? String(params[key]) : whole,
  );
}

/**
 * Resolve `key` against `catalog`, falling back to `fallback` (the English
 * catalog) when the active locale lacks it. A function value is called with
 * `arg`; a string value with an object `arg` gets {name} interpolation.
 */
export function translate(
  catalog: Catalog,
  fallback: Catalog,
  key: string,
  arg?: MessageArg,
): string {
  const entry = catalog[key] ?? fallback[key];
  if (entry === undefined) {
    if (import.meta.env?.DEV) {
      // eslint-disable-next-line no-console
      console.warn(`[i18n] missing message key: ${key}`);
    }
    return key;
  }
  if (typeof entry === 'function') return entry(arg);
  if (arg && typeof arg === 'object') return interpolate(entry, arg);
  return entry;
}
