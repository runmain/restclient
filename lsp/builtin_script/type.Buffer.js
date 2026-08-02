/**
 * @httpyac-type Buffer
 * Instance methods when a value is typed Buffer (e.g. randomBytes return).
 */
'use strict';

module.exports = {
  /**
   * @returns {string}
   */
  toString(encoding) {},
  /**
   * @returns {Buffer}
   */
  slice(start, end) {},
  /**
   * @returns {Buffer}
   */
  subarray(start, end) {},
  /**
   * @returns {number}
   */
  readUInt8(offset) {},
  /**
   * @returns {Buffer}
   */
  write(string, offset, encoding) {},
  length: null,
};
