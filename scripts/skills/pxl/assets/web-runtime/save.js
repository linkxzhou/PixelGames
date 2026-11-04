export function loadSave(key, fallback = null, storage = globalThis.localStorage) {
  try {
    const value = storage?.getItem(key);
    return value == null ? fallback : JSON.parse(value);
  } catch {
    return fallback;
  }
}

export function writeSave(key, value, storage = globalThis.localStorage) {
  if (!storage) return false;
  try {
    storage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}
