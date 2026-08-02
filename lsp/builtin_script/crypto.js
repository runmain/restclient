/**
 * Builtin Node-style `crypto` (curated). User `.script/crypto.js` overrides/extends.
 */
'use strict';

module.exports = {
  /**
   * @returns {Hmac}
   */
  createHmac(algorithm, key) {},
  /**
   * @returns {Hash}
   */
  createHash(algorithm) {},
  /**
   * @returns {Sign}
   */
  createSign(algorithm) {},
  /**
   * @returns {Verify}
   */
  createVerify(algorithm) {},
  /**
   * @returns {Buffer}
   */
  randomBytes(size) {},
  /**
   * @returns {string}
   */
  randomUUID() {},
  pbkdf2() {},
  /**
   * @returns {Buffer}
   */
  pbkdf2Sync() {},
  scrypt() {},
  /**
   * @returns {Buffer}
   */
  scryptSync() {},
  createCipheriv() {},
  createDecipheriv() {},
  /**
   * @returns {Buffer}
   */
  publicEncrypt() {},
  /**
   * @returns {Buffer}
   */
  privateDecrypt() {},
  /**
   * @returns {boolean}
   */
  timingSafeEqual() {},
  /**
   * @returns {string[]}
   */
  getHashes() {},
  /**
   * @returns {string[]}
   */
  getCiphers() {},
  constants: null,
};
