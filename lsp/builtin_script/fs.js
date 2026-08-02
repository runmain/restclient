/** Builtin Node `fs` (common sync APIs). */
'use strict';

module.exports = {
  readFileSync(path, encoding) {},
  writeFileSync(path, data) {},
  existsSync(path) {},
  readdirSync(path) {},
  statSync(path) {},
  mkdirSync(path, opts) {},
  readFile(path, cb) {},
  writeFile(path, data, cb) {},
};
