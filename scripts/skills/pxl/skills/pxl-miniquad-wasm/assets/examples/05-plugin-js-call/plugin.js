export function sum(bytes) {
  return bytes.reduce((total, value) => total + value, 0);
}
