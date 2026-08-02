/**
 * Builtin IntelliJ-style `client` API (httpyac).
 */
'use strict';

module.exports = {
  global: {
    set(key, value) {}, // client.global.set(key, value)
    get(key) {}, // client.global.get(key)
    isEmpty() {}, // client.global.isEmpty()
    clear(key) {}, // client.global.clear(key?)
    clearAll() {}, // client.global.clearAll()
  },
  test(name, fn) {}, // client.test(name, () => { … })
  assert(condition, message) {}, // client.assert(condition, message?)
  log() {}, // client.log(…)
  exit() {}, // client.exit() — stop further requests
  isInitial: null, // Whether this is the first request in a run
};
